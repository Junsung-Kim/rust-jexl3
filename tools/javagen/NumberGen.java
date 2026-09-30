// Generates JVM-oracle fixtures for src/java/number.rs and src/java/big_decimal.rs.
// Run: java -XX:-OmitStackTraceInFastThrow tools/javagen/NumberGen.java <outDir> [countMultiplier=1] [seed=20260930] [sections]
// (the flag matters: once JIT-compiled, a hot "/ by zero" otherwise becomes a message-less exception)
// Each output line is tab separated; strings are escaped with esc() (\\ and \\uXXXX for anything
// outside printable ASCII). Exceptions are written as EX:<class code>:<message> (see ex(); a null
// message is written as an empty message).
import java.io.*;
import java.math.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.function.*;

public class NumberGen {
    static SplittableRandom R;
    static int M;

    public static void main(String[] a) throws Exception {
        Path out = Paths.get(a[0]);
        M = a.length > 1 ? Integer.parseInt(a[1]) : 1;
        long seed = a.length > 2 ? Long.parseLong(a[2]) : 20260930L;
        Files.createDirectories(out);
        R = new SplittableRandom(seed);
        String only = a.length > 3 ? a[3] : "";
        if (only.isEmpty() || only.contains("double")) doubles(out.resolve("double.txt"), 6000 * M);
        if (only.isEmpty() || only.contains("float")) floats(out.resolve("float.txt"), 6000 * M);
        if (only.isEmpty() || only.contains("parsefp")) parseFp(out.resolve("parse_fp.txt"), 7000 * M);
        if (only.isEmpty() || only.contains("parseint")) parseInts(out.resolve("parse_int.txt"), 6000 * M);
        if (only.isEmpty() || only.contains("chardigit")) charDigits(out.resolve("char_digit.txt"));
        if (only.isEmpty() || only.contains("bdparse")) bdParse(out.resolve("bd_parse.txt"), 6000 * M);
        if (only.isEmpty() || only.contains("bdunary")) bdUnary(out.resolve("bd_unary.txt"), 1800 * M);
        if (only.isEmpty() || only.contains("bdbinary")) bdBinary(out.resolve("bd_binary.txt"), 1600 * M);
        if (only.isEmpty() || only.contains("edge")) edge(out.resolve("edge.txt"));
    }

    // ---------------------------------------------------------------- helpers
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

    // Exception classes are abbreviated: A = ArithmeticException, N = NumberFormatException,
    // I = IllegalArgumentException. Where the message echoes the current input string, the echo is
    // replaced by {s} (see CUR).
    static String CUR = null;

    static String ex(Throwable t) {
        String c = t instanceof ArithmeticException ? "A" : t instanceof NumberFormatException ? "N" : "I";
        String m = esc(t.getMessage());
        if (CUR != null && CUR.length() >= 2) m = m.replace(esc(CUR), "{s}");
        return "EX:" + c + ":" + m;
    }

    static String run(Supplier<Object> f) {
        try {
            Object o = f.get();
            if (o instanceof BigDecimal d) return d.toString();
            if (o instanceof Double d) return Long.toHexString(Double.doubleToRawLongBits(d));
            if (o instanceof Float d) return Integer.toHexString(Float.floatToRawIntBits(d));
            return String.valueOf(o);
        } catch (ArithmeticException | IllegalArgumentException e) {
            return ex(e);
        }
    }

    static PrintWriter open(Path p) throws IOException {
        return new PrintWriter(Files.newBufferedWriter(p, StandardCharsets.UTF_8));
    }

    static <T> T pick(T[] xs) { return xs[R.nextInt(xs.length)]; }

    static String digits(int n) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < n; i++) b.append((char) ('0' + R.nextInt(10)));
        return b.toString();
    }

    // ---------------------------------------------------------------- doubles
    static double randomDouble() {
        switch (R.nextInt(12)) {
            case 0: return Double.longBitsToDouble(R.nextLong());
            case 1: return Double.longBitsToDouble(R.nextLong(0, 1L << 52)) * (R.nextBoolean() ? 1 : -1); // subnormal
            case 2: { // power of ten neighbourhood
                double p = Double.parseDouble("1e" + R.nextInt(-325, 310));
                int k = R.nextInt(-3, 4);
                for (; k > 0; k--) p = Math.nextUp(p);
                for (; k < 0; k++) p = Math.nextDown(p);
                return p;
            }
            case 3: { // power of two neighbourhood
                double p = Math.scalb(1.0, R.nextInt(-1075, 1024));
                return R.nextBoolean() ? p : (R.nextBoolean() ? Math.nextUp(p) : Math.nextDown(p));
            }
            case 4: return Double.parseDouble(digits(R.nextInt(1, 18)) + "e" + R.nextInt(-330, 320));
            case 5: return Double.parseDouble(digits(R.nextInt(1, 8)) + "e" + R.nextInt(-10, 12));
            case 6: return R.nextLong() >> R.nextInt(64);
            case 7: return R.nextDouble() * Math.pow(10, R.nextInt(-8, 9));
            case 8: return Double.longBitsToDouble(R.nextLong(0, 16));
            case 9: return pick(new Double[]{0.0, -0.0, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY,
                    Double.MIN_VALUE, Double.MAX_VALUE, Double.MIN_NORMAL, -Double.MIN_VALUE, 1e7, 9999999.0, 1e-3,
                    9.999999999999999e-4, 0.001, 1e23, 2e23, 5e-324, 1.0, 0.1, 100.0, 1e22, 1e21, 4.35E-4, 1.0E-5,
                    Math.nextDown(1e7), Math.nextUp(1e7), Math.nextDown(1e-3), 1.2345678E7, 9.999999E-4,
                    2.2250738585072014E-308, 2.225073858507201E-308, 4.9E-324, 1.7976931348623157E308,
                    Double.longBitsToDouble(0x7ff8000000000001L), Double.longBitsToDouble(0xfff0000000000001L)});
            case 10: return (double) R.nextFloat() * (1 << R.nextInt(30));
            default: return Double.longBitsToDouble(R.nextLong(0x3c00000000000000L, 0x4400000000000000L));
        }
    }

    static void doubles(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                double d = randomDouble();
                StringBuilder b = new StringBuilder();
                b.append(Long.toHexString(Double.doubleToRawLongBits(d))).append('\t');
                b.append(Double.toString(d)).append('\t');
                b.append(Double.hashCode(d)).append('\t');
                b.append(run(() -> BigDecimal.valueOf(d))).append('\t');
                // new BigDecimal(d): fingerprint "scale/precision/hash" (full expansion is long)
                String exact;
                try {
                    BigDecimal e = new BigDecimal(d);
                    exact = e.scale() + "/" + e.precision() + "/" + e.hashCode()
                            + "/" + Integer.toHexString(Float.floatToRawIntBits(e.floatValue()))
                            + "/" + Long.toHexString(Double.doubleToRawLongBits(e.doubleValue()))
                            + (e.precision() < 40 ? "/" + e : "");
                } catch (NumberFormatException e) { exact = ex(e); }
                b.append(exact).append('\t');
                b.append(Long.hashCode(Double.doubleToRawLongBits(d)));
                w.println(b);
            }
        }
    }

    static float randomFloat() {
        switch (R.nextInt(9)) {
            case 0: return Float.intBitsToFloat(R.nextInt());
            case 1: return Float.intBitsToFloat(R.nextInt(0, 1 << 23)) * (R.nextBoolean() ? 1 : -1);
            case 2: {
                float p = Float.parseFloat("1e" + R.nextInt(-46, 40));
                int k = R.nextInt(-3, 4);
                for (; k > 0; k--) p = Math.nextUp(p);
                for (; k < 0; k++) p = Math.nextDown(p);
                return p;
            }
            case 3: return Math.scalb(1.0f, R.nextInt(-150, 128));
            case 4: return Float.parseFloat(digits(R.nextInt(1, 10)) + "e" + R.nextInt(-50, 40));
            case 5: return Float.parseFloat(digits(R.nextInt(1, 8)) + "e" + R.nextInt(-10, 12));
            case 6: return Float.intBitsToFloat(R.nextInt(0, 16));
            case 7: return pick(new Float[]{0.0f, -0.0f, Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY,
                    Float.MIN_VALUE, Float.MAX_VALUE, Float.MIN_NORMAL, 1e7f, 1e-3f, Math.nextDown(1e7f),
                    Math.nextDown(1e-3f), 1.0E10f, 3.4028235E38f, 1.4E-45f, 0.1f, 1.0f / 3,
                    Float.intBitsToFloat(0x7fc00001)});
            default: return (float) randomDouble();
        }
    }

    static void floats(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                float f = randomFloat();
                w.println(Integer.toHexString(Float.floatToRawIntBits(f)) + "\t" + Float.toString(f) + "\t"
                        + Float.hashCode(f) + "\t" + Double.toString((double) f));
            }
        }
    }

    // ---------------------------------------------------------------- parseDouble / parseFloat
    static final String[] WS = {" ", "\t", "\n", "\r", "\u0000", "\u0001", "\u001f", "\u000b", "\u000c"};

    static String randomFpString() {
        switch (R.nextInt(16)) {
            case 0: return Double.toString(randomDouble());
            case 1: return Float.toString(randomFloat());
            case 2: { // midpoint between adjacent doubles, optionally perturbed
                double d = Math.abs(randomDouble());
                if (!Double.isFinite(d) || d == Double.MAX_VALUE) d = 1.5;
                BigDecimal mid = new BigDecimal(d).add(new BigDecimal(Math.nextUp(d))).divide(BigDecimal.valueOf(2));
                String s = mid.toString();
                switch (R.nextInt(4)) {
                    case 0: return s;
                    case 1: return mid.round(new MathContext(R.nextInt(1, 30))).toString();
                    case 2: return mid.toPlainString() + "0000" + (R.nextBoolean() ? "1" : "");
                    default: return mid.subtract(mid.ulp()).toString();
                }
            }
            case 3: { // midpoint between adjacent floats
                float f = Math.abs(randomFloat());
                if (!Float.isFinite(f) || f == Float.MAX_VALUE) f = 1.5f;
                BigDecimal mid = new BigDecimal(f).add(new BigDecimal(Math.nextUp(f))).divide(BigDecimal.valueOf(2));
                switch (R.nextInt(3)) {
                    case 0: return mid.toString();
                    case 1: return mid.add(mid.ulp()).toString();
                    default: return mid.subtract(mid.ulp()).toString();
                }
            }
            case 4: { // hex float
                StringBuilder b = new StringBuilder();
                if (R.nextInt(3) == 0) b.append(R.nextBoolean() ? '-' : '+');
                b.append(R.nextBoolean() ? "0x" : "0X");
                int n1 = R.nextInt(0, 20), n2 = R.nextInt(0, 20);
                for (int i = 0; i < n1; i++) b.append("0123456789abcdefABCDEF".charAt(R.nextInt(22)));
                if (R.nextInt(4) != 0) b.append('.');
                for (int i = 0; i < n2; i++) b.append("0123456789abcdefABCDEF".charAt(R.nextInt(22)));
                if (R.nextInt(10) != 0) {
                    b.append(R.nextBoolean() ? 'p' : 'P');
                    if (R.nextBoolean()) b.append(R.nextBoolean() ? '-' : '+');
                    b.append(R.nextInt(R.nextBoolean() ? 20 : 1200));
                }
                if (R.nextInt(5) == 0) b.append("fFdD".charAt(R.nextInt(4)));
                return b.toString();
            }
            case 5: return Double.toHexString(randomDouble());
            case 6: return Double.toHexString(randomDouble()).replace("p", "0000p");
            case 7: { // random decimal with possibly huge digit counts / exponents
                StringBuilder b = new StringBuilder();
                if (R.nextBoolean()) b.append(R.nextBoolean() ? '-' : '+');
                b.append("0".repeat(R.nextInt(3)));
                b.append(digits(R.nextInt(0, R.nextInt(8) == 0 ? 900 : 25)));
                if (R.nextBoolean()) b.append('.').append(digits(R.nextInt(0, 30)));
                if (R.nextBoolean()) {
                    b.append(R.nextBoolean() ? 'e' : 'E');
                    if (R.nextBoolean()) b.append(R.nextBoolean() ? '-' : '+');
                    b.append(pick(new String[]{"" + R.nextInt(400), "" + R.nextInt(40), "" + R.nextLong(0, Long.MAX_VALUE),
                            "0000000000000000000" + R.nextInt(400), "", "2147483648", "9999999999", "10000000000"}));
                }
                return b.toString();
            }
            case 8: return pick(new String[]{"", " ", "NaN", "-NaN", "+NaN", "Infinity", "-Infinity", "+Infinity",
                    "nan", "infinity", "Inf", "NaNx", "Infinityy", " NaN ", "\tInfinity\n", "NaN f", "Infinityd",
                    ".", "e5", "1e", "1e+", "1e-", "+", "-", "-.5", "5.", "1_000", "1.2.3", "..1", "0x", "0x.p1", "0xp1",
                    "0x1", "0x1.", "0x.8", "0x1p", "0x1p+", "0x1.8p1", "0X1P-1074", "0x1p-1075", "0x1.0000000000001p-1075",
                    "0x1.fffffffffffff8p1023", "0x1.fffffffffffff7ffp1023", "0x.0000000000001p-1022", "1e400", "1e-400",
                    "1e99999999999", "0.0000e999999999999", "123e-2147483648", "1ee5", "1e5e5", "1f", "1d", "1F", "1D",
                    "1fd", "1.0f ", "0x1p1f", "0x1p1d", "0x1p1x", "  1  ", "1 1", "\u00001\u0000", "1 ", " 1",
                    "١", "１", "0", "-0", "+0.0e10", "0e-9999999999999", "00000", ".0", "0.", "-.0e1",
                    "2.4703282292062327E-324", "2.4703282292062328E-324", "4.9406564584124654E-324", "1.7976931348623158E308",
                    "1.7976931348623159E308", "3.4028235677973366E38", "3.4028236E38", "1.4012984643E-45", "7.006492321624085E-46",
                    "7.006492321624087E-46", "1.0E-45", "0.000000000000000000000000000000000000000000000701",
                    "179769313486231580793728971405303415079934132710037826936173778980444968292764750946649017977587207096330286416692887910946555547851940402630657488671505820681908902000708383676273854845817711531764475730270069855571366959622842914819860834936475292719074168444365510704342711559699508093042880177904174497791",
                    "179769313486231580793728971405303415079934132710037826936173778980444968292764750946649017977587207096330286416692887910946555547851940402630657488671505820681908902000708383676273854845817711531764475730270069855571366959622842914819860834936475292719074168444365510704342711559699508093042880177904174497792",
                    "0x0.0p0", "0x0p99999999999", "0x1p99999999999", "0x1p-99999999999", "1.", "+.e1", "0x1.p-1"});
            case 9: { // mutate a valid string with whitespace, sign, suffix
                String s = Double.toString(randomDouble());
                StringBuilder b = new StringBuilder();
                int k = R.nextInt(3);
                for (int i = 0; i < k; i++) b.append(pick(WS));
                b.append(s);
                if (R.nextInt(3) == 0) b.append("fFdD".charAt(R.nextInt(4)));
                k = R.nextInt(3);
                for (int i = 0; i < k; i++) b.append(pick(WS));
                return b.toString();
            }
            case 10: { // garbage insertion
                StringBuilder b = new StringBuilder(Double.toString(randomDouble()));
                int pos = R.nextInt(b.length() + 1);
                b.insert(pos, pick(new String[]{".", "e", "-", "+", "x", "p", "f", " ", "0x", "٠", "é", "😀", "E1"}));
                return b.toString();
            }
            case 11: if (R.nextInt(8) != 0) return digits(R.nextInt(1, 30)); { // very long input -> message truncation
                StringBuilder b = new StringBuilder(digits(R.nextInt(900, 1300)));
                b.insert(R.nextInt(b.length()), pick(new String[]{"x", "..", "-", "é", "😀"}));
                return b.toString();
            }
            case 12: return digits(R.nextInt(1, 20)) + "e-" + R.nextInt(300, 350);
            case 13: return digits(R.nextInt(1, 20)) + "e" + R.nextInt(280, 330);
            case 14: return "0." + "0".repeat(R.nextInt(0, R.nextInt(4) == 0 ? 400 : 20)) + digits(R.nextInt(1, 30));
            default: return digits(R.nextInt(1, 10)) + "." + digits(R.nextInt(0, 10)) + "e" + (R.nextInt(-50, 50));
        }
    }

    static void parseFp(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                String s = randomFpString();
                CUR = s;
                w.println(esc(s) + "\t" + run(() -> Double.parseDouble(s)) + "\t" + run(() -> Float.parseFloat(s)));
            }
        }
    }

    // ---------------------------------------------------------------- parseInt & co
    static final String UNI_DIGITS = "٠١٩۰۵०९০௧０９ＡＺａｚ၀꧐09²①Ⅻ༠１";

    static String randomIntString(int radix) {
        long v;
        switch (R.nextInt(14)) {
            case 0: v = R.nextInt(); break;
            case 1: v = R.nextLong(); break;
            case 2: v = pick(new Long[]{(long) Integer.MIN_VALUE, (long) Integer.MAX_VALUE, Long.MIN_VALUE, Long.MAX_VALUE,
                    Integer.MIN_VALUE - 1L, Integer.MAX_VALUE + 1L, 0L, -1L, 1L, 127L, 128L, -128L, -129L, 32767L, 32768L,
                    -32768L, -32769L, 255L, 256L, 65535L, 65536L});
                break;
            case 3: v = R.nextInt(-300, 300); break;
            case 4: v = R.nextInt(-70000, 70000); break;
            case 5: {
                BigInteger b = BigInteger.valueOf(Long.MAX_VALUE).add(BigInteger.valueOf(R.nextInt(-3, 4)));
                if (R.nextBoolean()) b = b.negate().subtract(BigInteger.ONE);
                return b.toString(radix);
            }
            case 6: { // big digits
                StringBuilder b = new StringBuilder();
                if (R.nextBoolean()) b.append(R.nextBoolean() ? '-' : '+');
                b.append("0".repeat(R.nextInt(0, 3)));
                int len = R.nextInt(1, 80);
                for (int i = 0; i < len; i++) b.append(Character.forDigit(R.nextInt(radix), radix));
                return b.toString();
            }
            case 7: return pick(new String[]{"", "+", "-", "++1", "--1", "+-1", "1-", "1+", " 1", "1 ", "0x10", "1_0",
                    "1.0", "1e3", "٣", "١٢٣", "-١٢٣", "１２３", "ＦＦ", "ｆｆ", "ZZ", "zz", "z", "²", "Ⅻ", "0-", "-0", "+0",
                    "00000000000000000000000000000000000000001", "-00000000000000000000000000000000000000000",
                    "۱۲۳", "၀၁", "٣٣٣٣٣٣٣٣٣٣٣٣", "12٣4", "1a", "A", "a", "ａ", "-Ｚ"});
            case 8: { // unicode digit splicing
                StringBuilder b = new StringBuilder(Long.toString(R.nextLong() >> R.nextInt(64), radix));
                int k = R.nextInt(1, 4);
                for (int i = 0; i < k; i++) b.insert(R.nextInt(b.length() + 1), UNI_DIGITS.charAt(R.nextInt(UNI_DIGITS.length())));
                return b.toString();
            }
            case 9: { // garbage
                StringBuilder b = new StringBuilder(Long.toString(R.nextInt(), radix));
                b.insert(R.nextInt(b.length() + 1), pick(new String[]{" ", "-", "+", ".", "g", "\u0000", "é", "😀", " "}));
                return b.toString();
            }
            default: v = R.nextLong() >> R.nextInt(64); break;
        }
        String s = Long.toString(v, radix);
        if (R.nextInt(6) == 0 && !s.startsWith("-")) s = "+" + s;
        if (R.nextInt(6) == 0) s = R.nextBoolean() ? s.toUpperCase() : s;
        return s;
    }

    static void parseInts(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                int radix = R.nextInt(5) < 3 ? 10 : (R.nextInt(40) == 0 ? pick(new Integer[]{0, 1, 37, 100}) : R.nextInt(2, 37));
                int rr = (radix < 2 || radix > 36) ? 10 : radix;
                String s = randomIntString(rr);
                CUR = s;
                String big = run(() -> { BigInteger b = new BigInteger(s, radix); return b.hashCode() + ""; });
                w.println(esc(s) + "\t" + radix + "\t" + run(() -> Integer.parseInt(s, radix)) + "\t"
                        + run(() -> Long.parseLong(s, radix)) + "\t" + run(() -> Short.parseShort(s, radix)) + "\t"
                        + run(() -> Byte.parseByte(s, radix)) + "\t" + big);
            }
        }
    }

    // Runs of BMP chars with Character.digit(c, 36) != -1: "start end valueAtStart" with value
    // increasing by one along the run.
    static void charDigits(Path p) throws IOException {
        try (PrintWriter w = open(p)) {
            int c = 0;
            while (c < 0x10000) {
                int v = Character.digit((char) c, 36);
                if (v < 0) { c++; continue; }
                int s = c, sv = v;
                while (c + 1 < 0x10000 && Character.digit((char) (c + 1), 36) == v + 1) { c++; v++; }
                w.println(Integer.toHexString(s) + " " + Integer.toHexString(c) + " " + sv);
                c++;
            }
        }
    }

    // ---------------------------------------------------------------- BigDecimal
    static final RoundingMode[] RM = RoundingMode.values();

    static MathContext randomMc() {
        int prec;
        switch (R.nextInt(8)) {
            case 0: prec = 0; break;
            case 1: prec = pick(new Integer[]{7, 16, 34}); break;
            case 2: prec = R.nextInt(1, 4); break;
            case 3: prec = R.nextInt(17, 21); break;
            case 4: prec = R.nextInt(35, 60); break;
            default: prec = R.nextInt(1, 40); break;
        }
        return new MathContext(prec, pick(RM));
    }

    static int randomScale() {
        switch (R.nextInt(20)) {
            case 0: return Integer.MAX_VALUE - R.nextInt(0, 40);
            case 1: return Integer.MIN_VALUE + R.nextInt(0, 40);
            case 2: case 3: return R.nextInt(-400, 400);
            default: return R.nextInt(-40, 41);
        }
    }

    static BigInteger randomUnscaled() {
        switch (R.nextInt(12)) {
            case 0: return BigInteger.ZERO;
            case 1: return BigInteger.valueOf(R.nextInt(-10, 11));
            case 2: { // long boundary
                BigInteger b = BigInteger.valueOf(Long.MAX_VALUE).add(BigInteger.valueOf(R.nextInt(-2, 3)));
                return R.nextBoolean() ? b : b.negate();
            }
            case 3: { // tie-ish: d...d5 * 10^k
                BigInteger b = new BigInteger(digits(R.nextInt(1, 20)) + "5" + "0".repeat(R.nextInt(0, 4)));
                return R.nextBoolean() ? b : b.negate();
            }
            case 4: { // 9999..
                BigInteger b = BigInteger.TEN.pow(R.nextInt(1, 40)).subtract(BigInteger.valueOf(R.nextInt(0, 2)));
                return R.nextBoolean() ? b : b.negate();
            }
            case 5: return BigInteger.TEN.pow(R.nextInt(0, 30)).multiply(BigInteger.valueOf(R.nextInt(-9, 10)));
            case 6: return new BigInteger(digits(R.nextInt(40, 90)) + "1").negate();
            case 7: return BigInteger.valueOf(R.nextLong() >> R.nextInt(64));
            default: {
                BigInteger b = new BigInteger("1" + digits(R.nextInt(0, 30)));
                return R.nextBoolean() ? b : b.negate();
            }
        }
    }

    static BigDecimal randomBd() {
        if (R.nextInt(20) == 0) return new BigDecimal((R.nextDouble() - 0.5) * Math.pow(10, R.nextInt(-5, 6)));
        return new BigDecimal(randomUnscaled(), randomScale());
    }

    static boolean moderate(BigDecimal d) {
        return Math.abs((long) d.scale()) < 5000;
    }

    static String bdString() {
        switch (R.nextInt(14)) {
            case 0: return randomBd().toString();
            case 1: { BigDecimal d = randomBd(); return moderate(d) ? d.toPlainString() : d.toString(); }
            case 2: return randomBd().toEngineeringString();
            case 3: return pick(new String[]{"", "-", "+", ".", "..", "1..2", "-.5", "+5.", ".e1", "1E1E1", "1e", "1e+",
                    "1e-", "1ea", "1e5.5", "e5", "1e2147483647", "1e-2147483648", "1e2147483648", "1e-2147483649",
                    "1e+0000000000005", "1e+00000000000000000000005", "1e12345678901", "1e1234567890", "0e2147483648",
                    "0.0e-2147483648", "-0", "+0.000", "00001.2300", "١٢.٣", "１２３", "12٣e١",
                    "1e５", "1_0", "0x10", "1 ", " 1", "NaN", "Infinity", "1.2.3", "-1.23E-10", "123456789012345678",
                    "1234567890123456789", "12345678901234567.8", "1234567890123456789.", "+1234567890123456789",
                    "-12345678901234567890e-5", "1234567890123456789012345678901234567890", "0000000000000000000000000",
                    "000000000000000000000000.00000001", "12345678901234567890x", "12345678901234567890.1.2",
                    "12345678901234567890e", "12345678901234567890e+", "1234567890123456789e12345678901",
                    "123456789012345678901234567890e-2147483648", "9223372036854775807", "-9223372036854775808",
                    "9223372036854775808", "0.1e-2147483647", "10e-2147483648", "123E+2147483647", "1.5", "2.5", "-2.5",
                    "0.5", "-0.5", "1.45", "1.55", "9.95", "99.5", "999999999999999999999999999999999999.5"});
            case 4: { // unicode / garbage splice
                StringBuilder b = new StringBuilder(randomBd().toString());
                b.insert(R.nextInt(b.length() + 1), pick(new String[]{"x", ".", "e", "E", "-", "+", " ", "٣", "０", "é",
                        "😀", "²", "Ⅻ"}));
                return b.toString();
            }
            case 5: return digits(R.nextInt(1, 40)) + "." + digits(R.nextInt(0, 40)) + "e" + R.nextInt(-50, 50);
            case 6: return (R.nextBoolean() ? "-" : "") + digits(R.nextInt(1, 25));
            default: {
                StringBuilder b = new StringBuilder();
                if (R.nextBoolean()) b.append(R.nextBoolean() ? '-' : '+');
                b.append(digits(R.nextInt(0, 25)));
                if (R.nextBoolean()) b.append('.').append(digits(R.nextInt(0, 25)));
                if (R.nextBoolean()) b.append(R.nextBoolean() ? 'e' : 'E').append(R.nextBoolean() ? "-" : (R.nextBoolean() ? "+" : "")).append(R.nextInt(0, 60));
                return b.toString();
            }
        }
    }

    static void bdParse(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                String s = bdString();
                CUR = s;
                MathContext mc = randomMc();
                w.println(esc(s) + "\t" + run(() -> new BigDecimal(s)) + "\t" + mc.getPrecision() + "\t" + mc.getRoundingMode()
                        + "\t" + run(() -> new BigDecimal(s, mc)) + "\t" + run(() -> new BigDecimal(s).hashCode()));
            }
        }
    }

    static void bdUnary(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                BigDecimal a = randomBd();
                MathContext mc = randomMc();
                int k = R.nextInt(5) == 0 ? randomScale() : R.nextInt(-45, 46);
                RoundingMode rm = pick(RM);
                int pw = R.nextInt(10) == 0 ? pick(new Integer[]{-1, 1000000000, 999999999, -999999999, -1000000000, 0, 1})
                        : R.nextInt(-12, 13);
                boolean mod = moderate(a);
                List<String> f = new ArrayList<>();
                f.add(a.toString());
                f.add(mc.getPrecision() + "");
                f.add(mc.getRoundingMode() + "");
                f.add(k + "");
                f.add(rm + "");
                f.add(pw + "");
                f.add(mod ? a.toPlainString() : "-");
                f.add(a.toEngineeringString());
                f.add(a.hashCode() + "");
                f.add(a.precision() + "");
                f.add(a.signum() + "");
                f.add(run(() -> a.round(mc)));
                f.add(run(() -> a.setScale(k, rm)));
                f.add(run(() -> a.stripTrailingZeros()));
                f.add(run(() -> a.longValueExact()));
                f.add(run(() -> a.intValueExact()));
                f.add(run(() -> a.longValue()));
                f.add(run(() -> a.intValue()));
                f.add(run(() -> a.doubleValue()));
                f.add(run(() -> a.floatValue()));
                f.add(run(() -> a.toBigInteger()));
                f.add(run(() -> a.toBigIntegerExact()));
                boolean smallPow = a.precision() < 60 && pw >= 0 && pw < 20
                        || a.unscaledValue().abs().compareTo(BigInteger.ONE) <= 0 || pw < 0 || pw > 999999999;
                String pws = smallPow ? run(() -> a.pow(pw)) : "-";
                f.add(pws.length() > 120 ? "-" : pws);
                MathContext pmc = mc.getPrecision() == 0 ? new MathContext(R.nextInt(1, 30), mc.getRoundingMode()) : mc;
                f.add(pmc.getPrecision() + "");
                f.add(run(() -> a.pow(pw, pmc)));
                f.add(a.negate().toString());
                f.add(a.abs().toString());
                f.add(run(() -> a.movePointLeft(k)));
                f.add(run(() -> a.movePointRight(k)));
                f.add(a.ulp().toString());
                f.add(mc.toString());
                w.println(String.join("\t", f));
            }
        }
    }

    static void bdBinary(Path p, int n) throws IOException {
        try (PrintWriter w = open(p)) {
            for (int i = 0; i < n; i++) {
                BigDecimal a = randomBd();
                BigDecimal b = R.nextInt(10) == 0 ? a : randomBd();
                if (R.nextInt(8) == 0 && moderate(a)) b = a.add(BigDecimal.ONE.movePointLeft(R.nextInt(0, 5)));
                final BigDecimal bb = b;
                MathContext mc = randomMc();
                int k = R.nextInt(-45, 46);
                RoundingMode rm = pick(RM);
                // Avoid ops whose exact result would need a 10^n with 10^6 < n < 10^9 (slow in both JVM and Rust).
                long sd = Math.abs((long) a.scale() - bb.scale());
                boolean align = sd < 5000 || sd > 800_000_000L;
                List<String> f = new ArrayList<>();
                f.add(a.toString());
                f.add(bb.toString());
                f.add(mc.getPrecision() + "");
                f.add(mc.getRoundingMode() + "");
                f.add(k + "");
                f.add(rm + "");
                f.add(align ? run(() -> a.add(bb)) : "-");
                f.add(align ? run(() -> a.subtract(bb)) : "-");
                f.add(run(() -> a.multiply(bb)));
                f.add(align || mc.getPrecision() > 0 ? run(() -> a.add(bb, mc)) : "-");
                f.add(align || mc.getPrecision() > 0 ? run(() -> a.subtract(bb, mc)) : "-");
                f.add(run(() -> a.multiply(bb, mc)));
                f.add(run(() -> a.divide(bb, mc)));
                f.add(run(() -> a.divide(bb)));
                f.add(run(() -> a.divide(bb, k, rm)));
                f.add(run(() -> a.divideToIntegralValue(bb, mc)));
                f.add(run(() -> a.remainder(bb, mc)));
                f.add(run(() -> a.remainder(bb)));
                f.add(run(() -> a.divideToIntegralValue(bb)));
                f.add(a.compareTo(bb) + "");
                f.add(a.equals(bb) + "");
                f.add(a.max(bb).toString());
                f.add(a.min(bb).toString());
                w.println(String.join("\t", f));
            }
        }
    }

    // Hand-picked cases for paths the random sections cannot reach cheaply.
    // Line: op \t args... \t expected
    static void edge(Path p) throws IOException {
        try (PrintWriter w = open(p)) {
            // BigInteger.pow overflow: bitsToShift overflow, and the final-size check.
            for (String[] c : new String[][]{{"8", "999999999"}, {"-8", "999999999"}, {"12", "999999999"},
                    {"3", "0"}, {"0", "999999999"}, {"1", "999999999"}, {"-1", "999999999"}, {"-1", "999999998"}}) {
                BigDecimal a = new BigDecimal(c[0]);
                int n = Integer.parseInt(c[1]);
                w.println("pow\t" + c[0] + "\t" + c[1] + "\t" + run(() -> a.pow(n)));
            }
            // (odd << k)^n, where only the exact result size overflows 2^31 bits.
            for (int[] c : new int[][]{{7, 2147481, 1000}, {7, 2147480, 1000}, {5, 2147482, 1000}, {3, 1073741, 2000}}) {
                BigDecimal a = new BigDecimal(BigInteger.valueOf(c[0]).shiftLeft(c[1]));
                String r = run(() -> { BigDecimal x = a.pow(c[2]); return x.precision() + "/" + x.hashCode(); });
                w.println("powshift\t" + c[0] + "\t" + c[1] + "\t" + c[2] + "\t" + r);
            }
            // pow(n, mc) with precision 0 is pow(n).
            for (String[] c : new String[][]{{"1.5", "7"}, {"-2.5", "3"}, {"1.5", "-1"}, {"0.1", "12"}}) {
                BigDecimal a = new BigDecimal(c[0]);
                int n = Integer.parseInt(c[1]);
                w.println("powmc0\t" + c[0] + "\t" + c[1] + "\t" + run(() -> a.pow(n, MathContext.UNLIMITED)));
            }
            // Character.digit with an out-of-range radix.
            for (int radix : new int[]{-1, 0, 1, 37, 100}) {
                w.println("digit\t" + radix + "\t" + Character.digit('5', radix) + "," + Character.digit('a', radix));
            }
        }
    }
}
