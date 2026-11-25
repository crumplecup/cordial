r"""Aggregate the salsa::stale events of a log (stdin, ANSI stripped) by memo, by what changed, and by pair.
Usage: sed 's/\x1b\[[0-9;]*m//g' LOG | python3 stale.py"""
import sys,re,collections
pair=collections.Counter(); memo=collections.Counter(); cause=collections.Counter()
for l in sys.stdin:
    if 'salsa::stale' not in l: continue
    m=re.search(r'memo="?([^" ]*)"?.*?changed="?([^" ]*)"?\s+changed_key',l)
    if not m: continue
    pair[(m.group(1),m.group(2))]+=1; memo[m.group(1)]+=1; cause[m.group(2)]+=1
print('stale events:',sum(pair.values()))
print('-- by memo (query):'); [print(f'  {n:7d} {k}') for k,n in memo.most_common(14)]
print('-- by what changed (ingredient):'); [print(f'  {n:7d} {k}') for k,n in cause.most_common(14)]
print('-- top (memo <- changed):'); [print(f'  {n:7d} {a} <- {b}') for (a,b),n in pair.most_common(18)]
