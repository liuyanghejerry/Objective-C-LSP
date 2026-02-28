//! Protocol conformance checking and stub generation.
//!
//! When a class declares `@interface Foo <Bar, Baz>`, every required method
//! in `Bar` and `Baz` must be implemented in `@implementation Foo`.
//! This module detects missing implementations and generates stubs.

/// A required method that has not been implemented.
#[derive(Debug, Clone)]
pub struct MissingMethod {
    pub protocol: String,
    pub selector: String,
    pub is_class_method: bool,
    pub signature: String, // e.g. "- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip"
}

/// Generate the stub source text for a list of missing methods.
pub fn generate_stubs(methods: &[MissingMethod]) -> String {
    methods
        .iter()
        .map(|m| {
            format!(
                "{signature} {{\n    // TODO: implement {selector}\n}}\n",
                signature = m.signature,
                selector = m.selector,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
