#!/bin/sh
# Builds the oracle runner against the official jars from Maven Central (via ~/.m2).
set -e
cd "$(dirname "$0")"
M2=${M2_REPO:-$HOME/.m2/repository}
JEXL=$M2/org/apache/commons/commons-jexl3/3.2.1/commons-jexl3-3.2.1.jar
LOG=$M2/commons-logging/commons-logging/1.2/commons-logging-1.2.jar
if [ ! -f "$JEXL" ] || [ ! -f "$LOG" ]; then
  mvn -q dependency:get -Dartifact=org.apache.commons:commons-jexl3:3.2.1
  mvn -q dependency:get -Dartifact=commons-logging:commons-logging:1.2
fi
JAVA_HOME=${JAVA_HOME:-/usr/lib/jvm/java-25-amazon-corretto.aarch64}
rm -rf target && mkdir -p target/classes
"$JAVA_HOME/bin/javac" -nowarn -encoding UTF-8 -cp "$JEXL:$LOG" -d target/classes $(find src -name '*.java')
printf '#!/bin/sh\nexec "%s/bin/java" -Xss16m -cp "%s:%s:%s" rustjexl.oracle.Oracle "$@"\n' \
  "$JAVA_HOME" "$(pwd)/target/classes" "$JEXL" "$LOG" > target/oracle
chmod +x target/oracle
