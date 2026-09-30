/** Writes src/java/character_data.rs: Character.getType and getDirectionality as runs, from the JVM.
 *
 *   java tools/javagen/CharacterData.java > src/java/character_data.rs
 */
public class CharacterData {
    public static void main(String[] a) {
        StringBuilder b = new StringBuilder();
        b.append("// Generated from the JVM (Corretto ").append(System.getProperty("java.version"))
         .append(") by walking every code point: Character.getType and\n")
         .append("// Character.getDirectionality as runs of equal value. Do not edit; regenerate.\n\n");
        emit(b, "TYPE", false);
        emit(b, "DIRECTIONALITY", true);
        b.append("\n// outside 0..=0x10FFFF: getType(int) is ").append(Character.getType(-1)).append(" and ")
         .append(Character.getType(0x110000)).append(", getDirectionality(int) is ")
         .append(Character.getDirectionality(-1)).append(" and ").append(Character.getDirectionality(0x110000)).append("\n");
        System.out.print(b);
    }
    static void emit(StringBuilder b, String name, boolean dir) {
        StringBuilder runs = new StringBuilder();
        int last = Integer.MIN_VALUE, n = 0;
        for (int cp = 0; cp <= 0x10FFFF; cp++) {
            int v = dir ? Character.getDirectionality(cp) : Character.getType(cp);
            if (v != last) { runs.append("(0x").append(Integer.toHexString(cp)).append(", ").append(v).append("),"); n++; last = v;
                if (n % 8 == 0) runs.append("\n    "); }
        }
        b.append("pub(crate) static ").append(name).append(": [(u32, i8); ").append(n).append("] = [\n    ")
         .append(runs).append("\n];\n");
    }
}
