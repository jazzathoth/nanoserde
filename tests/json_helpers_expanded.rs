//! Readable equivalent of Fields/Key/Parser generation for a one-field struct.
#![cfg(feature = "json")]
use nanoserde::{DeJson, DeJsonErr, DeJsonHelpers, DeJsonScope};

#[derive(Debug, PartialEq, DeJsonHelpers)]
struct Entry {
    count: u8,
}

impl DeJson for Entry {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        nanoserde::fields_for!(Entry);
        nanoserde::parser_for!(Entry);
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            count: fields.count.ok_or_else(|| scope.err_nf("count"))?,
        })
    }
}

struct ExpandedEntry {
    count: u8,
}
impl DeJson for ExpandedEntry {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        struct Fields {
            count: Option<u8>,
        }
        #[allow(non_camel_case_types)]
        enum Key<'a> {
            count(&'a mut Option<u8>),
            __nanoserde_uninhabited(
                core::convert::Infallible,
                core::marker::PhantomData<&'a mut Fields>,
            ),
        }
        impl Fields {
            const NAMES: &'static [&'static str] = &["count"];
            fn new() -> Self {
                Self { count: None }
            }
            fn match_next_key<'a>(
                &'a mut self,
                scope: &mut DeJsonScope<'_, '_>,
            ) -> Result<Option<Key<'a>>, DeJsonErr> {
                let Some(key) = scope.next_key(Self::NAMES)? else {
                    return Ok(None);
                };
                match key {
                    "count" => {
                        if self.count.is_some() {
                            return Err(scope.err_parse("duplicate field count"));
                        }
                        Ok(Some(Key::count(&mut self.count)))
                    }
                    _ => unreachable!(),
                }
            }
        }
        struct Parser;
        impl Parser {
            fn parse_field(key: Key<'_>, scope: &mut DeJsonScope<'_, '_>) -> Result<(), DeJsonErr> {
                match key {
                    Key::count(destination) => *destination = Some(u8::de_json(scope)?),
                    Key::__nanoserde_uninhabited(never, _) => match never {},
                }
                Ok(())
            }
        }
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            count: fields.count.ok_or_else(|| scope.err_nf("count"))?,
        })
    }
}

#[test]
fn generated_and_expanded_helpers_agree() {
    for input in [
        r#"{"count":7}"#,
        "{}",
        r#"{"unknown":7}"#,
        r#"{"count":1,"count":2}"#,
        r#"{"count":256}"#,
        r#"{"count":1,}"#,
    ] {
        let generated = Entry::deserialize_json(input).map(|entry| entry.count);
        let expanded = ExpandedEntry::deserialize_json(input).map(|entry| entry.count);
        match (generated, expanded) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(a), Err(b)) => assert_eq!(format!("{a:?}"), format!("{b:?}")),
            _ => panic!("implementations disagree for {input}"),
        }
    }
}
