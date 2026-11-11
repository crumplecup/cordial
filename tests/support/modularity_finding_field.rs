//! Reads one named field off a finding.

use cordial::{Finding, MapFindingSink};

pub fn field(finding: &dyn Finding, name: &str) -> Option<String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields()
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
