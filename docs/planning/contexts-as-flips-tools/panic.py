#!/usr/bin/env python3
"""Read the log of a flipped run that panicked on an attribute id, and say why, by joining its events.

    RUST_LOG='cordial::semantic::reach=debug,cordial::semantic::resolve=debug,cordial::semantic::flip=debug,\
cordial::semantic::harvest=debug,cordial::semantic::contexts=info,cordial::attrs=error,cordial::panic=error,\
cordial::segment=info,salsa::stale=debug' cordial open -p <project> > run.log 2>&1
    sed 's/\\x1b\\[[0-9;]*m//g' run.log | python3 panic.py

It answers: what the failing call is; whether its crate was flipped and has the call in its map (before, in the context,
after); how the failing function entered the walk of reachable facts and who listed it; what the memos that should have been
out of date read. `python3 panic.py --selftest` checks the parsing on lines made by hand."""
import re
import sys

ORDER = {
    "the cfg": None,
    "about to be expanded": ["call", "krate", "krate_name", "attr_index", "cfg_atoms_that_are_not_the_target", "in_file", "item"],
    "what a crate has": ["phase", "krate", "krate_id", "flipped", "flags", "modules", "attr_macro_calls", "calls_removed_since_ordinary", "calls_added_since_ordinary", "calls"],
    "a function the reachable facts": ["root", "function", "called_by", "krate", "krate_id"],
    "the direct facts of a function are asked for": ["function", "krate", "krate_id"],
    "the same-crate functions a function calls": ["function", "label", "krate", "krate_id", "made_by_a_macro", "targets", "target_ids"],
    "the evidence of a function is asked for": ["function"],
    "were computed, and this is what they read": ["function", "found", "reads"],
    "was resolved to a definition": ["description", "found", "found_is", "made_how", "revision", "reads"],
}


def fields(line, key):
    """The named fields of an event line, in the order the event says them, whose values may hold spaces."""
    names = ORDER[key]
    rest = line.split(key, 1)[1]
    rest = rest.split(" ", 1)[1] if " " in rest and not rest.lstrip().startswith(names[0] + "=") else rest
    out, pos = {}, rest.find(names[0] + "=")
    if pos < 0:
        return out
    for i, name in enumerate(names):
        start = rest.find(name + "=", pos)
        if start < 0:
            break
        start += len(name) + 1
        end = len(rest)
        if i + 1 < len(names):
            nxt = rest.find(" " + names[i + 1] + "=", start)
            end = nxt if nxt >= 0 else len(rest)
        out[name] = rest[start:end].strip().strip('"')
        pos = end
    return out


def read(lines):
    events, flip_at = [], None
    for n, line in enumerate(lines):
        for key in ORDER:
            if ORDER[key] and key in line:
                events.append((n, key, fields(line, key)))
        if flip_at is None and "what a crate has" in line and "phase=\"flipped" in line:
            flip_at = n
    return events, flip_at


def hexid(text):
    found = re.findall(r"0x[0-9a-f]+|Id\(([0-9a-f]+)\)", text or "")
    return {("0x" + f.lstrip("0x") if f else f) for f in found}


def main(lines):
    events, flip_at = read(lines)
    by = lambda key: [(n, f) for n, k, f in events if k == key]
    fail = by("about to be expanded")
    panic = next((l for l in lines if "cordial::panic" in l and "a panic" in l), "")
    stack = "\n".join(lines[lines.index(panic):lines.index(panic) + 12]) if panic else ""
    ids = dict(re.findall(r"(\w+)\((?:\w*)Id\(([0-9a-f]+)\)\)", stack))
    print("== the failing call")
    if not fail:
        print("  no `about to be expanded` event: the run did not panic on an attribute id, or the build lacks the event")
    for n, f in fail[:1]:
        print("  call", f.get("call"), "of crate", f.get("krate_name"), f.get("krate"), "attr", f.get("attr_index"))
        print("  its cfg flags:", f.get("cfg_atoms_that_are_not_the_target"))
        print("  item:", (f.get("item") or "")[:120].replace("\n", " "))
        kid = (re.search(r"Id\(([0-9a-f]+)\)", f.get("krate", "")) or [None, ""])[1]
        print("\n== that crate in each phase (flipped? flags, calls the map declares, is the call among them?)")
        for _, g in by("what a crate has"):
            if g.get("krate_id", "").replace("0x", "") == kid:
                calls = set(g.get("calls", "").split())
                print("  %-44s flipped=%s flags=[%s] modules=%s calls=%s removed=%s added=%s has_the_call=%s" % (
                    g.get("phase", "")[:44], g.get("flipped"), g.get("flags"), g.get("modules"),
                    g.get("attr_macro_calls"), g.get("calls_removed_since_ordinary"),
                    g.get("calls_added_since_ordinary"), f.get("call") in calls))
    print("\n== the stack at the panic:", ids or "none")
    failing = ids.get("for_body_") or ids.get("InferenceResult < 'db >::for_body_")
    print("\n== how the failing function entered the walk")
    if failing:
        for n, f in by("a function the reachable facts"):
            if failing in f.get("function", ""):
                print("  line", n, "root", f.get("root"), "called_by", f.get("called_by"), "krate", f.get("krate"),
                      "(after the flip)" if flip_at is not None and n > flip_at else "(before the flip)")
        for n, f in by("the same-crate functions a function calls"):
            if failing in f.get("target_ids", ""):
                print("  listed by", f.get("label"), f.get("function"), "of", f.get("krate"), "at line", n,
                      "(after the flip)" if flip_at is not None and n > flip_at else "(before the flip)")
        for n, f in by("the same-crate functions a function calls"):
            if failing in f.get("target_ids", ""):
                lister = f.get("function")
                recomputed = [m for m, g in by("were computed, and this is what they read")
                              if g.get("function") == lister and flip_at is not None and m > flip_at]
                print("  its caller's direct facts were recomputed in the flipped phase:", bool(recomputed),
                      "(if not, the saved value from the ordinary pass still lists it)")
        for n, f in by("the direct facts of a function are asked for"):
            if failing in f.get("function", ""):
                print("  executed direct facts at line", n, f)
        for n, f in by("were computed, and this is what they read"):
            if failing in f.get("function", ""):
                print("  what its direct facts read:", f.get("reads"))
    print("\n== resolve (what it read), for the item of the failing call")
    name = (re.search(r"fn (\w+)", (fail[0][1].get("item") or "")) or [None, None])[1] if fail else None
    for n, f in by("was resolved to a definition"):
        if name and "fn %s|" % name in f.get("description", ""):
            print("  line", n, "revision", f.get("revision"), "made", f.get("made_how"), "reads:", f.get("reads"))


def selftest():
    sample = [
        'T INFO cordial::semantic::flip: what a crate has: its cfg, whether the context flipped it, and the attribute macro calls its map declares phase="flipped +kani" krate=core krate_id=0x1fe5a flipped=Some(true) flags=feature=a kani modules=3 attr_macro_calls=2 calls_removed_since_ordinary=1 calls_added_since_ordinary=0 calls=0x34b9c 0x34b9d',
        'T ERROR cordial::attrs: an attribute macro call whose attribute its crate\'s cfg does not have, about to be expanded call=0x34b9c krate=Crate(Id(1fe5a)) krate_name=core attr_index=0 cfg_atoms_that_are_not_the_target=feature=a kani in_file=MacroFile item=pub fn witness_exports() {}',
        'T ERROR cordial::panic: a panic thread="main" location=x queries=query stacktrace:',
        '   0: MacroCallId::macro_arg_(Id(34b9c))',
        "   1: InferenceResult < 'db >::for_body_(Id(6c4ee))",
        '   2: function_evidence(Id(cda67))',
        'T DEBUG cordial::semantic::reach: a function the reachable facts of a function go on to read root=FunctionId(Id(1)) function=FunctionId(Id(6c4ee)) called_by=None krate=core krate_id=0x1fe5a',
    ]
    events, flip_at = read(sample)
    got = {k: f for _, k, f in events}
    assert got["about to be expanded"]["call"] == "0x34b9c" and got["about to be expanded"]["cfg_atoms_that_are_not_the_target"] == "feature=a kani", got
    assert got["what a crate has"]["flags"] == "feature=a kani" and got["what a crate has"]["calls"] == "0x34b9c 0x34b9d", got
    assert "6c4ee" in got["a function the reachable facts"]["function"], got
    main(sample)
    print("selftest ok")


if __name__ == "__main__":
    selftest() if "--selftest" in sys.argv else main(sys.stdin.read().split("\n"))
