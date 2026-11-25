r"""Attribute salsa::stale events to the region (cordial::segment event) that ends after them. Stdin: a log with both filters on.
Usage: sed 's/\x1b\[[0-9;]*m//g' LOG | python3 byregion.py"""
import sys,re,collections
pending=collections.Counter(); out=collections.defaultdict(collections.Counter)
for l in sys.stdin:
    if 'salsa::stale' in l:
        m=re.search(r'memo="?([^" ]*)"?.*?changed="?([^" ]*)"?\s+changed_key',l)
        if m: pending[(m.group(1),m.group(2))]+=1
    elif 'cordial::segment' in l:
        m=re.search(r'label=(\S+)',l)
        if m and pending:
            lab=m.group(1)
            for k,n in pending.items(): out[lab][k]+=n
            pending=collections.Counter()
for lab,c in sorted(out.items(), key=lambda kv:-sum(kv[1].values()))[:6]:
    print(f'{sum(c.values()):6d}  region ending: {lab}')
    for (a,b),n in c.most_common(5): print(f'        {n:6d} {a} <- {b}')
