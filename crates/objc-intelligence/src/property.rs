//! @property rename coordination.
//!
//! A single `@property (nonatomic, copy) NSString *name` generates:
//!   - a getter selector `name`
//!   - a setter selector `setName:`
//!   - an ivar `_name` (via `@synthesize`)
//!   - dot-syntax accesses `obj.name = x` / `x = obj.name`
//!
//! clangd#81775 (open since Feb 2024) does not coordinate these.
//! This module computes all the identifiers that must be renamed together.

/// All rename targets derived from a single `@property` name.
#[derive(Debug, Clone)]
pub struct PropertyRenameTargets {
    pub property_name: String,
    /// The getter selector (default: same as property name).
    pub getter: String,
    /// The setter selector (default: `set<Name>:`).
    pub setter: String,
    /// The synthesized ivar (default: `_<name>`).
    pub ivar: String,
}

impl PropertyRenameTargets {
    /// Compute default targets from a property name.
    ///
    /// Pass custom getter/setter if `@property (getter=isHidden)` is used.
    pub fn from_property(
        name: &str,
        custom_getter: Option<&str>,
        custom_setter: Option<&str>,
    ) -> Self {
        let getter = custom_getter.unwrap_or(name).to_owned();
        let setter = custom_setter.map(str::to_owned).unwrap_or_else(|| {
            let mut s = String::from("set");
            let mut chars = name.chars();
            if let Some(first) = chars.next() {
                s.extend(first.to_uppercase());
                s.push_str(chars.as_str());
            }
            s.push(':');
            s
        });
        let ivar = format!("_{name}");

        Self {
            property_name: name.to_owned(),
            getter,
            setter,
            ivar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_targets() {
        let t = PropertyRenameTargets::from_property("name", None, None);
        assert_eq!(t.getter, "name");
        assert_eq!(t.setter, "setName:");
        assert_eq!(t.ivar, "_name");
    }

    #[test]
    fn custom_getter() {
        let t = PropertyRenameTargets::from_property("hidden", Some("isHidden"), None);
        assert_eq!(t.getter, "isHidden");
        assert_eq!(t.setter, "setHidden:");
    }
}
