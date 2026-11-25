"""Compare the regions of runs from their metrics.json: queries executed, memos re-validated, eviction and
superseded memos. See docs/planning/contexts-as-flips-handoff.md."""
import json,re,collections
def load(p):
    m=json.load(open(p)); v=m['values']; n=m.get('notes',{})
    rows=collections.defaultdict(dict)
    for k,val in v.items():
        mm=re.match(r'segment\.(.+)\.(micros|queries|verified_memos|keyed_reads|keyed_touches)$',k)
        if mm: rows[mm.group(1)][mm.group(2)]=val
    return rows,v,n
import sys
# Usage: python3 cmp.py label=path/to/metrics.json label=path/to/metrics.json ... (in the order to print)
files=dict(arg.split('=',1) for arg in sys.argv[1:])
data={k:load(p) for k,p in files.items()}
regs=['projection_evidence','projection_ordinary','projection_contexts','projection_contexts_context_harvest','projection_contexts_context_observability','projection_finish_flips','projection_leaves','projection_inventories']
print(f"{'region (queries executed)':46s}"+''.join(f"{k:>16s}" for k in files))
for r in regs:
    print(f"{r[11:]:46s}"+''.join(f"{data[k][0].get(r,{}).get('queries','-')!s:>16}" for k in files))
print('\nverified memos:')
for r in regs:
    print(f"{r[11:]:46s}"+''.join(f"{data[k][0].get(r,{}).get('verified_memos','-')!s:>16}" for k in files))
print('\neviction / superseded:')
for key in ['eviction.cfg_context.deferred_released','eviction.body_with_source_map.dropped','eviction.attrflags_docs.dropped','eviction.blocks']:
    print(f"{key:46s}"+''.join(f"{data[k][1].get(key,'-')!s:>16}" for k in files))
print('\nkeyed reads/touches (now):')
for k in ['copies(now)','flips(now)']:
    rows=data[k][0]; print(k, sum(d.get('keyed_reads',0) for r,d in rows.items() if r=='projection'), sum(d.get('keyed_touches',0) for r,d in rows.items() if r=='projection'))
