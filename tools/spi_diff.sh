#!/bin/sh
# groups the current spi_oracle mismatches by (op, receiver type, name)
set -e
cd "$(dirname "$0")/.."
rm -f /tmp/spi_dump.jsonl
SPI_DUMP=/tmp/spi_dump.jsonl cargo test --test spi_oracle >/dev/null 2>&1 || true
python3 - "$@" <<'PY'
import json, collections, sys
rows=[json.loads(l) for l in open('/tmp/spi_dump.jsonl')]
g=collections.Counter()
ex={}
for r in rows:
    t=(r.get('target') or {}).get('t') if isinstance(r.get('target'),dict) else None
    k=(r['op'], t, r['name'])
    g[k]+=1
    ex.setdefault(k,r)
for k,v in g.most_common():
    r=ex[k]
    print(v, k)
    print('   args', json.dumps(r['args'])[:110])
    print('   want', json.dumps(r['want'])[:190])
    print('   got ', json.dumps(r['got'])[:190])
PY
