trait Probe {}

trait Loader {}

struct View<'a> {
    probes: &'a [&'static dyn Probe],
}

struct Registry {
    loaders: &'static [&'static dyn Loader],
}

struct Plugin(&'static dyn Probe);

struct Mixed {
    name: &'static str,
    probes: &'static [&'static dyn Probe],
}

struct ForeignDyn {
    err: &'static dyn std::error::Error,
}
