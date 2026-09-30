// Oracle generator for src/java/regex.rs (java.util.regex.Pattern / Matcher /
// PatternSyntaxException, plus the String.matches/split/replaceAll/replaceFirst shims).
//
//   java tools/javagen/RegexGen.java tests/data/java_regex
//
// Deterministic (fixed seed). Writes, one case per line, TAB-separated:
//
//   compile.txt  flags | pattern | OK
//                flags | pattern | P | index | description | getMessage()
//   ops.txt      flags | pattern | input | replacement | matches | find
//                      | split(0) | split(2) | split(-1) | replaceAll | replaceFirst
//   strops.txt   input | regex | replacement | String.matches | split(0) | split(2)
//                      | split(-1) | replaceAll | replaceFirst
//   props.txt    \p{...} body | caseInsensitive(0|1) | lo-hi,lo-hi,... (hex code points)
//   quote.txt    input | Pattern.quote | Matcher.quoteReplacement
//
// Strings are escaped per esc(): `\` -> `\\`, printable ASCII verbatim, everything else
// (UTF-16 code unit) -> `backslash-u + 4 hex digits`.  String arrays are `count` then the elements, joined
// with a raw U+0001 (0x01) (which escaped text can never contain).
// Exceptions are `EX:<tag>:<message>`; tag P = PatternSyntaxException (then the payload is
// `index:description`), I = IllegalArgumentException, X = IndexOutOfBoundsException.
import java.io.*;
import java.util.*;
import java.util.regex.*;

public class RegexGen {

    static final long SEED = 20260930L;
    static final char SEP = (char) 1;

    public static void main(String[] args) throws Exception {
        File dir = new File(args[0]);
        dir.mkdirs();
        // Optional second argument scales the random campaigns up for a local soak run.
        int scale = args.length > 1 ? Integer.parseInt(args[1]) : 1;
        List<Integer> flagSets = flagSets();
        List<String> patterns = patterns();
        List<String> inputs = inputs();

        try (PrintWriter w = writer(new File(dir, "compile.txt"))) {
            Random r = new Random(SEED);
            for (String p : patterns)
                for (int f : flagSets)
                    compileCase(w, p, f);
            for (int i = 0; i < 20000 * scale; i++)
                compileCase(w, fuzzPattern(r), r.nextInt(4) == 0 ? randFlags(r) : 0);
        }
        try (PrintWriter w = writer(new File(dir, "ops.txt"))) {
            Random r = new Random(SEED + 1);
            // Every (pattern, flag set) pair, against a rotating slice of the input pool
            // (the full cross product is ~25MB of fixtures for no extra coverage).
            int n = inputs.size();
            for (int i = 0; i < patterns.size(); i++) {
                for (int j = 0; j < flagSets.size(); j++) {
                    int f = flagSets.get(j);
                    opsCase(w, patterns.get(i), f, "", pickRepl(r));
                    opsCase(w, patterns.get(i), f, "abc", pickRepl(r));
                    for (int k = 0; k < 6; k++)
                        opsCase(w, patterns.get(i), f, inputs.get((i * 7 + j * 13 + k * 11) % n),
                                pickRepl(r));
                }
            }
            for (int i = 0; i < 6000 * scale; i++)
                opsCase(w, fuzzPattern(r), r.nextInt(4) == 0 ? randFlags(r) : 0,
                        inputs.get(r.nextInt(inputs.size())), pickRepl(r));
        }
        try (PrintWriter w = writer(new File(dir, "strops.txt"))) {
            Random r = new Random(SEED + 2);
            for (String re : strRegexes())
                for (String in : inputs) strCase(w, in, re, pickRepl(r));
            for (int i = 0; i < 4000 * scale; i++)
                strCase(w, inputs.get(r.nextInt(inputs.size())), fuzzPattern(r), pickRepl(r));
        }
        try (PrintWriter w = writer(new File(dir, "props.txt"))) {
            for (String name : propNames())
                for (int ci = 0; ci < 2; ci++) propCase(w, name, ci == 1);
        }
        try (PrintWriter w = writer(new File(dir, "quote.txt"))) {
            for (String s : quoteInputs())
                w.println(esc(s) + "\t" + esc(Pattern.quote(s)) + "\t"
                        + esc(Matcher.quoteReplacement(s)));
        }
    }

    static PrintWriter writer(File f) throws IOException {
        return new PrintWriter(new BufferedWriter(
                new OutputStreamWriter(new FileOutputStream(f), "UTF-8")));
    }

    // ---------------------------------------------------------------- escaping

    static String esc(String s) {
        if (s == null) return "";
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\') b.append("\\\\");
            else if (c >= 0x20 && c < 0x7f) b.append(c);
            else b.append(String.format("\\u%04X", (int) c));
        }
        return b.toString();
    }

    static String arr(String[] a) {
        StringBuilder b = new StringBuilder().append(a.length);
        for (String s : a) b.append(SEP).append(esc(s));
        return b.toString();
    }

    static String ex(Throwable t) {
        if (t instanceof PatternSyntaxException) {
            PatternSyntaxException p = (PatternSyntaxException) t;
            return "EX:P:" + p.getIndex() + ":" + esc(p.getDescription());
        }
        String tag = t instanceof IndexOutOfBoundsException ? "X"
                   : t instanceof IllegalArgumentException ? "I" : "?";
        return "EX:" + tag + ":" + esc(t.getMessage());
    }

    interface Op<T> { T get() throws Exception; }

    static String str(Op<String> f) {
        try { return esc(f.get()); } catch (Throwable t) { return ex(t); }
    }

    static String strArr(Op<String[]> f) {
        try { return arr(f.get()); } catch (Throwable t) { return ex(t); }
    }

    static String bool(Op<Boolean> f) {
        try { return f.get() ? "T" : "F"; } catch (Throwable t) { return ex(t); }
    }

    // ---------------------------------------------------------------- cases

    static void compileCase(PrintWriter w, String p, int flags) {
        try {
            Pattern pat = Pattern.compile(p, flags);
            if (!pat.pattern().equals(p) || !pat.toString().equals(p))
                throw new AssertionError("pattern() mismatch");
            w.println(flags + "\t" + esc(p) + "\tOK");
        } catch (PatternSyntaxException e) {
            w.println(flags + "\t" + esc(p) + "\tP\t" + e.getIndex() + "\t"
                    + esc(e.getDescription()) + "\t" + esc(e.getMessage()));
        }
    }

    static void opsCase(PrintWriter w, String p, int flags, final String in, final String repl) {
        final Pattern pat;
        try { pat = Pattern.compile(p, flags); } catch (PatternSyntaxException e) { return; }
        w.println(flags + "\t" + esc(p) + "\t" + esc(in) + "\t" + esc(repl)
                + "\t" + bool(new Op<Boolean>() { public Boolean get() { return pat.matcher(in).matches(); } })
                + "\t" + bool(new Op<Boolean>() { public Boolean get() { return pat.matcher(in).find(); } })
                + "\t" + strArr(new Op<String[]>() { public String[] get() { return pat.split(in, 0); } })
                + "\t" + strArr(new Op<String[]>() { public String[] get() { return pat.split(in, 2); } })
                + "\t" + strArr(new Op<String[]>() { public String[] get() { return pat.split(in, -1); } })
                + "\t" + str(new Op<String>() { public String get() { return pat.matcher(in).replaceAll(repl); } })
                + "\t" + str(new Op<String>() { public String get() { return pat.matcher(in).replaceFirst(repl); } }));
    }

    static void strCase(PrintWriter w, final String in, final String re, final String repl) {
        w.println(esc(in) + "\t" + esc(re) + "\t" + esc(repl)
                + "\t" + bool(new Op<Boolean>() { public Boolean get() { return in.matches(re); } })
                + "\t" + strArr(new Op<String[]>() { public String[] get() { return in.split(re, 0); } })
                + "\t" + strArr(new Op<String[]>() { public String[] get() { return in.split(re, 2); } })
                + "\t" + strArr(new Op<String[]>() { public String[] get() { return in.split(re, -1); } })
                + "\t" + str(new Op<String>() { public String get() { return in.replaceAll(re, repl); } })
                + "\t" + str(new Op<String>() { public String get() { return in.replaceFirst(re, repl); } }));
    }

    static void propCase(PrintWriter w, String name, boolean ci) {
        int flags = ci ? Pattern.CASE_INSENSITIVE : 0;
        Pattern pat;
        try {
            pat = Pattern.compile("\\p{" + name + "}", flags);
        } catch (PatternSyntaxException e) {
            w.println(esc(name) + "\t" + (ci ? 1 : 0) + "\t" + ex(e));
            return;
        }
        StringBuilder b = new StringBuilder();
        int start = -1;
        for (int cp = 0; cp <= Character.MAX_CODE_POINT + 1; cp++) {
            boolean hit = cp <= Character.MAX_CODE_POINT
                    && pat.matcher(new String(Character.toChars(cp))).matches();
            if (hit && start < 0) start = cp;
            else if (!hit && start >= 0) {
                if (b.length() > 0) b.append(',');
                b.append(Integer.toHexString(start)).append('-')
                 .append(Integer.toHexString(cp - 1));
                start = -1;
            }
        }
        w.println(esc(name) + "\t" + (ci ? 1 : 0) + "\t" + b);
    }

    // ---------------------------------------------------------------- corpora

    static List<Integer> flagSets() {
        int CI = Pattern.CASE_INSENSITIVE, M = Pattern.MULTILINE, S = Pattern.DOTALL;
        int D = Pattern.UNIX_LINES, X = Pattern.COMMENTS, L = Pattern.LITERAL;
        int U = Pattern.UNICODE_CASE, UU = Pattern.UNICODE_CHARACTER_CLASS;
        List<Integer> out = new ArrayList<Integer>();
        for (int f : new int[] {0, CI, M, S, D, X, L, CI | U, UU, CI | UU, M | S, M | D,
                                S | D, CI | M | S, L | CI})
            out.add(f);
        return out;
    }

    static int randFlags(Random r) {
        int[] all = {Pattern.CASE_INSENSITIVE, Pattern.MULTILINE, Pattern.DOTALL,
                     Pattern.UNIX_LINES, Pattern.COMMENTS, Pattern.LITERAL,
                     Pattern.UNICODE_CASE, Pattern.UNICODE_CHARACTER_CLASS};
        int f = 0;
        int n = r.nextInt(3);
        for (int i = 0; i < n; i++) f |= all[r.nextInt(all.length)];
        return f;
    }

    static String pickRepl(Random r) {
        String[] repls = {"X", "", "[$0]", "$1", "$2", "$0$0", "<${name}>", "a\\$b", "\\\\",
                          "$", "\\", "${}", "${1a}", "$12", "$-", "x$1y$2z", "\\n", "\\$",
                          "😀", "가", "$10", "${user}-${host}"};
        return repls[r.nextInt(repls.length)];
    }

    /** Curated patterns: every construct the port claims to support, plus many invalid ones. */
    static List<String> patterns() {
        return new ArrayList<String>(Arrays.asList(
            // --- literals, empty, metacharacter soup
            "", "a", "abc", "a|b", "a|", "|a", "||", "abc|def|", "\\", "a\\", "^", "$", "^$",
            ".", "..", ".*", ".+", ".?", "a*", "a+b", "a?b", "ab*", "ab+c",
            "]", "}", "a]b", "a}b", "[]", "[^]", "[a]", "[]a]", "[^]a]",
            // --- quantifiers
            "a{2}", "a{2,}", "a{2,4}", "a{0,0}", "a{0,1}", "a{,2}", "a{}", "a{2,1}", "a{2",
            "a{2,", "a{99999999999}", "a{1,99999999999}", "(ab){2,3}", "(?:ab){2,3}",
            "a*?", "a+?", "a??", "a{2,4}?", "a*+", "a++", "a?+", "a{2,4}+", "(a|b)*+",
            "*a", "+a", "?a", "a**", "a{1}{2}", "(?:a)*+",
            // --- groups
            "(a)", "(a)(b)", "(a(b))", "(?:a)", "(?i)a", "(?i:a)", "(?-i)a", "(?i-s:a)",
            "(?im-sx:ab)", "(?#comment)a", "(?<name>a)", "(?<name>a)\\k<name>",
            "(?<n1>a)(?<n2>b)", "(?<name>a)(?<name>b)", "(?<1a>x)", "(?<>x)", "(?<na me>x)",
            "(?<name a)", "(?=a)", "(?!a)", "(?<=a)", "(?<!a)", "(?<=a*)", "(?<=ab|cde)",
            "(?>ab)", "(?>a|ab)c", "(a", "a)", "(", ")", "(?", "(?$)", "(?@)", "(?P<n>a)",
            "(?'n'a)", "(?)", "(?:", "(?=", "((((((a))))))",
            // --- backreferences
            "(a)\\1", "(a)(b)\\2\\1", "\\1", "(a)\\12",
            "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)\\11",
            "\\k", "\\k<", "\\k<n>", "(?<n>a)\\k<n>", "(?<n>a)\\k<m>", "\\0", "\\00", "\\012",
            "\\0777", "\\08", "\\0'", "\\o", "\\8",
            // --- escapes
            "\\t", "\\n", "\\r", "\\f", "\\a", "\\e", "\\x41", "\\x4", "\\xZZ", "\\x{1F600}",
            "\\x{110000}", "\\x{41", "\\x{}", "\\u0041", "\\u00", "\\uD83D\\uDE00", "\\uZZZZ",
            "\\cA", "\\c", "\\cZ", "\\c@", "\\Q", "\\Qa.b\\E", "\\Qa.b", "\\Q\\E", "a\\Qb*\\Ec*",
            "\\E", "\\Q[\\E", "\\N{LATIN SMALL LETTER A}", "\\N{NOSUCHNAME}", "\\N", "\\N{",
            "\\d", "\\D", "\\s", "\\S", "\\w", "\\W", "\\h", "\\H", "\\v", "\\V", "\\R", "\\X",
            "\\b", "\\B", "\\A", "\\z", "\\Z", "\\G", "\\b{g}", "\\b{x}", "\\C", "\\i", "\\y",
            "\\-", "\\#", "\\ ", "\\/",
            // --- character classes
            "[abc]", "[^abc]", "[a-z]", "[a-z0-9]", "[^a-z]", "[z-a]", "[a-]", "[-a]",
            "[a\\-z]", "[\\d]", "[\\D]", "[\\w\\s]", "[\\x41-\\x5A]", "[\\0101]", "[\\cA-\\cZ]",
            "[a-z&&[^aeiou]]", "[a-z&&[def]]", "[abc[def]]", "[a-d[m-p]]", "[a-z&&[^m-p]]",
            "[&&]", "[a&&]", "[&&a]", "[a&&b&&c]", "[[a][b]]", "[^[a][b]]", "[a[^b]]",
            "[\\Qab\\E]", "[\\p{Alpha}]", "[\\p{Alpha}&&[^a]]", "[\\P{Alpha}]",
            "[a", "[a-", "[\\", "[^", "[]]", "[\\Q]\\E]", "[.]", "[$^]", "[|]", "[(]",
            "[가-힣]", "[😀-😏]", "[\\x{1F600}-\\x{1F64F}]",
            "[--a]", "[a--b]", "[~~a]", "[\\v]", "[\\v-\\x20]", "[\\b]",
            // --- properties
            "\\p{Alpha}", "\\p{Digit}", "\\p{Alnum}", "\\p{Punct}", "\\p{Graph}", "\\p{Print}",
            "\\p{Blank}", "\\p{Cntrl}", "\\p{XDigit}", "\\p{Space}", "\\p{Lower}", "\\p{Upper}",
            "\\p{ASCII}", "\\p{L}", "\\p{Lu}", "\\p{Ll}", "\\p{N}", "\\p{Nd}", "\\p{P}",
            "\\p{S}", "\\p{Z}", "\\p{C}", "\\p{Cc}", "\\p{Sc}", "\\p{Mn}", "\\p{LC}",
            "\\pL", "\\pN", "\\PL", "\\P{L}", "\\p{IsAlphabetic}", "\\p{IsLetter}",
            "\\p{IsDigit}", "\\p{IsLowercase}", "\\p{IsUppercase}", "\\p{IsWhite_Space}",
            "\\p{IsHex_Digit}", "\\p{IsL}", "\\p{IsLatin}", "\\p{IsGreek}", "\\p{IsHangul}",
            "\\p{InGreek}", "\\p{InBasicLatin}", "\\p{InHangul_Syllables}", "\\p{InNoSuchBlock}",
            "\\p{javaLowerCase}", "\\p{javaUpperCase}", "\\p{javaDigit}", "\\p{javaLetter}",
            "\\p{javaLetterOrDigit}", "\\p{javaWhitespace}", "\\p{javaSpaceChar}",
            "\\p{javaAlphabetic}", "\\p{javaTitleCase}", "\\p{javaDefined}",
            "\\p{javaISOControl}", "\\p{javaJavaIdentifierStart}", "\\p{javaJavaIdentifierPart}",
            "\\p{javaIdentifierIgnorable}", "\\p{javaIdeographic}",
            "\\p{gc=Lu}", "\\p{general_category=Lu}", "\\p{sc=Latin}", "\\p{script=Greek}",
            "\\p{blk=Greek}", "\\p{block=Basic_Latin}", "\\p{bogus=Lu}", "\\p{gc=Nope}",
            "\\p", "\\p{", "\\p{}", "\\pZ", "\\p{Nope}", "\\p{L", "\\P{",
            // --- anchors and line handling
            "^a$", "(?m)^a$", "(?s).", "(?d)^a$", "(?m)(?d)^a$", "^.*$", "a$", "\\Aa\\z",
            "a\\Z", "\\R+", "\\v+", "\\h+", "(?m)^", "(?m)$", "\\G", "\\Ga",
            // --- comments mode
            "a b", "a # comment\nb", "[a b]", "a\\ b", "(?x)a b", "(?x)a # c\nb",
            // --- realistic JEXL patterns
            "\\d+", "[0-9]+", "^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}$",
            "\\s*,\\s*", "(?i)hello.*world", ".*\\.txt", "[A-Z]{3}-\\d{4}",
            "^(\\d{1,3}\\.){3}\\d{1,3}$", "\\bfoo\\b", "(\\w+)@(\\w+)",
            "(?<user>\\w+)@(?<host>[\\w.]+)", "a(?=b)", "a(?!b)", "(?<=a)b", "(?<!a)b",
            "한국어+", "[가-힣]+", "😀+", "[\\p{IsHangul}]+",
            "^\\s*$", "\\p{Punct}+", "(a|b)*abb",
            // --- coverage of rarely-taken branches in the port
            "a\\p{L}", "a\\X", "a\\b{g}", "\\b{gx}", "\\b{", "a\\d", "a\\1", "(a)a\\1",
            "[\\p{Cs}]", "[^\\p{Cs}]", "[\\p{Cs}&&[a]]", "\\p{Cs}", "\\P{Cs}", "[a&&\\p{Cs}]",
            "\\uD800", "[\\uD800-\\uDFFF]", "[\\uDC00]", "\\uD83D", "\\uD83Dx", "\\uD83D\\u0041",
            "(?x)a#c", "(?x)a # comment", "(?x)[a#b]", "(?x) a b # tail",
            "[a-z&&[b][c]]", "[a-z&&[b]c]", "[a&&b[c]]", "[a&&[b]&&[c]]",
            "[a[b&&c]]", "[[a&&b]c]", "[x[a-z&&[^m-p]]]", "[[a-z&&[^aeiou]][0-9]]",
            "[^[a-z&&[^m]]x]", "[[a&&b]&&[c&&d]]",
            "(?u)a", "(?c)a", "(?U)a", "(?-u)a", "(?-c)a", "(?-x)a", "(?-U)a", "(?-m)a",
            "(?-s)a", "(?-d)a", "(?idmsux)a", "(?idmsuxU-idmsuxU:a)", "(?-q)a",
            "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)",
            "[\u00E9]", "[\u00DF]", "[\u0130]", "[\u212A]", "[\u017F]", "[\u00B5]",
            "[\u00FF]", "[\u00C5]", "[\u0100-\u0200]", "\u00E9", "\u00DF", "\u212A",
            "\u0130", "\u017F", "[\u00E9-\u00FF]",
            "\u00DFx", "x\u00DF", "\u00DF\u00DF", "\u1E9Ex", "x\u0130", "\u0130x"
        ));
    }

    static List<String> strRegexes() {
        return Arrays.asList(
            // String.split fastpath candidates (single non-meta char, or backslash + non-alnum)
            ",", ":", "-", "a", " ", "\t", "\n", "/", "=", "#", "~", "!", "가",
            // single meta chars -> no fastpath
            ".", "$", "|", "(", ")", "[", "{", "^", "?", "*", "+", "\\",
            // two-char backslash forms
            "\\.", "\\-", "\\|", "\\$", "\\\\", "\\[", "\\{", "\\ ", "\\#", "\\/",
            "\\d", "\\w", "\\s", "\\D", "\\Q", "\\E", "\\q", "\\1",
            // ordinary regexes
            "\\s+", "\\s*,\\s*", "[,;]", "o", "", "()", "b*", "(?=a)", "x?", "\\b"
        );
    }

    static List<String> inputs() {
        return Arrays.asList(
            "", "a", "ab", "abc", "A", "AB", "aBc", "abcabc", "aaa", "b", "xyz",
            "a.b", "a*b", "a|b", "1", "12", "123", "a1b2c3", "  ", " a b ", "\ta\tb\t",
            "boo:and:foo", "a,b,,c,", ",,,", ",a,", "one two  three",
            "\n", "\r", "\r\n", "a\nb", "a\r\nb", "a\rb", "ab", "a b", "a b",
            "a\nb\nc", "line1\nline2\n", "\n\n", "a\n", "\na",
            "한국어", "가각갂", "한 a 국",
            "😀", "😀😁", "a😀b",
            "user@example.com", "ABC-1234", "192.168.0.1", "foo bar foo", "file.txt",
            "éÉ", "İı", "ſ", "K", "straße",
            "0x1F", "  trim  ", "$1", "a$b", "\\", "a\\b", "hello world", "HELLO WORLD",
            "\u00E9", "\u00C9", "abcdefghijkl", "a\u0301", "\u1E9E", "\u00B5\u039C",
            "\u1E9Ex", "x\u1E9E", "\u00DFx", "\u1E9E\u1E9E", "x\u0130", "i\u0307x"
        );
    }

    static List<String> quoteInputs() {
        return Arrays.asList("", "a", "a.b", "\\E", "a\\Eb", "\\E\\E", "a\\Eb\\Ec", "\\Q\\E",
                "$1", "a$b", "\\", "\\\\", "$", "a\\$b", "😀", "가", "\n\t");
    }

    static List<String> propNames() {
        return Arrays.asList(
            "Lower", "Upper", "ASCII", "Alpha", "Digit", "Alnum", "Punct", "Graph", "Print",
            "Blank", "Cntrl", "XDigit", "Space",
            "L", "Lu", "Ll", "Lt", "Lm", "Lo", "M", "Mn", "Me", "Mc", "N", "Nd", "Nl", "No",
            "Z", "Zs", "Zl", "Zp", "C", "Cc", "Cf", "Co", "Cs", "Cn", "P", "Pd", "Ps", "Pe",
            "Pc", "Po", "Pi", "Pf", "S", "Sm", "Sc", "Sk", "So", "LC", "LD", "L1", "all",
            "IsAlphabetic", "IsLetter", "IsDigit", "IsLowercase", "IsUppercase", "IsTitlecase",
            "IsWhite_Space", "IsHex_Digit", "IsNoncharacter_Code_Point", "IsAssigned",
            "IsControl", "IsPunctuation", "IsIdeographic", "IsJoin_Control", "IsAlnum",
            "IsLatin", "IsGreek", "IsHangul", "IsHan", "IsCyrillic", "IsCommon", "IsHiragana",
            "InBasicLatin", "InGreek", "InHangul_Syllables", "InCJK_Unified_Ideographs",
            "InEmoticons", "InLatin-1 Supplement", "InArabic",
            "javaLowerCase", "javaUpperCase", "javaTitleCase", "javaAlphabetic",
            "javaIdeographic", "javaDigit", "javaDefined", "javaLetter", "javaLetterOrDigit",
            "javaJavaIdentifierStart", "javaJavaIdentifierPart", "javaUnicodeIdentifierStart",
            "javaUnicodeIdentifierPart", "javaIdentifierIgnorable", "javaSpaceChar",
            "javaWhitespace", "javaISOControl", "javaMirrored",
            "gc=Lu", "gc=Nd", "general_category=L", "sc=Latin", "script=Greek",
            "blk=Greek", "block=Basic_Latin"
        );
    }

    // ---------------------------------------------------------------- fuzzing

    static final String[] FRAGS = {
        "a", "b", "z", "0", "9", ".", "*", "+", "?", "|", "^", "$", "(", ")", "[", "]", "{",
        "}", "-", "&", "&&", "\\", "\\\\", "\\d", "\\D", "\\w", "\\W", "\\s", "\\S", "\\b",
        "\\B", "\\A", "\\z", "\\Z", "\\G", "\\Q", "\\E", "\\R", "\\h", "\\v", "\\p{L}",
        "\\p{Alpha}", "\\P{L}", "\\pL", "\\x41", "\\x{1F600}", "\\u0041",
        "\\cA", "\\0101", "\\1", "\\2", "\\k<n>", "(?:", "(?i)", "(?-i)", "(?i:", "(?=", "(?!",
        "(?<=", "(?<!", "(?>", "(?<n>", "(?#c)", "{2}", "{2,}", "{2,4}", "{,2}", "*?", "+?",
        "*+", "++", "[a-z]", "[^a]", "[a&&b]", "#", " ", "\n", "가", "😀",
        " ", "\\p{In", "\\p{Is", "}", ":", "=", "<", ">", "!", "'", "\"", "~", "%",
        "\\Qx\\E"
    };

    static String fuzzPattern(Random r) {
        int n = 1 + r.nextInt(6);
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < n; i++) {
            if (r.nextInt(8) == 0) b.append((char) r.nextInt(0x80));
            else b.append(FRAGS[r.nextInt(FRAGS.length)]);
        }
        return b.toString();
    }
}
