#!/bin/sh
# Regenerates tests/data/jxlt/api_cases.jsonl and api_expected.jsonl from the real jar.
# Needs oracle/build.sh to have run first (it compiles Json/Hosts/Oracle).
set -e
cd "$(dirname "$0")/.."
M2=${M2_REPO:-$HOME/.m2/repository}
JEXL=$M2/org/apache/commons/commons-jexl3/3.2.1/commons-jexl3-3.2.1.jar
LOG=$M2/commons-logging/commons-logging/1.2/commons-logging-1.2.jar
JAVA_HOME=${JAVA_HOME:-/usr/lib/jvm/java-25-amazon-corretto.aarch64}
[ -d oracle/target/classes ] || oracle/build.sh
CP="oracle/target/classes:$JEXL:$LOG"
"$JAVA_HOME/bin/javac" -nowarn -encoding UTF-8 -cp "$CP" -d oracle/target/classes \
    tests/data/jxlt/gen/JxltGen.java
"$JAVA_HOME/bin/java" -Xss16m -cp "$CP" rustjexl.oracle.JxltGen \
    tests/data/jxlt/api_cases.jsonl tests/data/jxlt/api_expected.jsonl
