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

#[cfg(test)]
mod tests {
    use super::*;

    fn method(selector: &str, signature: &str, is_class_method: bool) -> MissingMethod {
        MissingMethod {
            protocol: "FakeProtocol".to_owned(),
            selector: selector.to_owned(),
            signature: signature.to_owned(),
            is_class_method,
        }
    }

    #[test]
    fn empty_list_yields_empty_string() {
        assert_eq!(generate_stubs(&[]), "");
    }

    #[test]
    fn single_instance_method_stub() {
        let m = method("viewDidLoad", "- (void)viewDidLoad", false);
        let out = generate_stubs(&[m]);
        assert!(out.contains("- (void)viewDidLoad {"), "stub header missing: {out}");
        assert!(out.contains("// TODO: implement viewDidLoad"), "TODO comment missing: {out}");
        assert!(out.contains('}'), "closing brace missing: {out}");
    }

    #[test]
    fn single_class_method_stub() {
        let m = method("+sharedInstance", "+ (instancetype)sharedInstance", true);
        let out = generate_stubs(&[m]);
        assert!(out.contains("+ (instancetype)sharedInstance {"), "class method header missing: {out}");
        assert!(out.contains("// TODO: implement +sharedInstance"), "TODO comment missing: {out}");
    }

    #[test]
    fn multiple_methods_joined_by_blank_line() {
        let methods = vec![
            method("foo", "- (void)foo", false),
            method("bar", "- (void)bar", false),
        ];
        let out = generate_stubs(&methods);
        // Both stubs present.
        assert!(out.contains("- (void)foo {"), "foo stub missing: {out}");
        assert!(out.contains("- (void)bar {"), "bar stub missing: {out}");
        // The two stubs are separated by a blank line (join("\n")).
        assert!(out.contains("\n\n"), "expected blank line between stubs: {out}");
    }

    #[test]
    fn compound_selector_stub() {
        let m = method(
            "tableView:didSelectRowAtIndexPath:",
            "- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip",
            false,
        );
        let out = generate_stubs(&[m]);
        assert!(
            out.contains("// TODO: implement tableView:didSelectRowAtIndexPath:"),
            "compound selector TODO missing: {out}"
        );
    }
}
