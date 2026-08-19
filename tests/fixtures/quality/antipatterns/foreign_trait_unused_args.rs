struct Walker;

impl Visit for Walker {
    fn visit_expr_closure(&mut self, _node: u32) {}
}

struct Qualified;

impl syn::visit::Visit for Qualified {
    fn visit_expr_closure(&mut self, _node: u32) {}
}

trait Local {
    fn hook(&self, arg: u32);
}

struct Mine;

impl Local for Mine {
    fn hook(&self, _arg: u32) {}
}

fn inherent(_z: i32) {}
