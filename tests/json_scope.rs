#![cfg(feature = "json")]
use nanoserde::{fields_for, parser_for, DeJson, DeJsonErr, DeJsonHelpers, DeJsonScope};

#[derive(Debug, PartialEq, DeJsonHelpers)]
struct Person {
    name: String,
    age: u8,
}
impl Person {
    fn new(name: String, age: u8) -> Self {
        Self { name, age }
    }
}
impl DeJson for Person {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(Person);
        parser_for!(Person);
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            match key {
                Key::name(destination) => {
                    let name = scope.parse_string()?;
                    if name.is_empty() {
                        return Err(scope.err_parse("name cannot be empty"));
                    }
                    *destination = Some(name);
                }
                other => Parser::parse_field(other, scope)?,
            }
        }
        let name = fields.name.ok_or_else(|| scope.err_nf("name"))?;
        let age = fields.age.ok_or_else(|| scope.err_nf("age"))?;
        Ok(Person::new(name, age))
    }
}

#[derive(Debug, PartialEq, DeJsonHelpers)]
struct Group<T: DeJson> {
    members: Vec<T>,
    leader: Option<Box<T>>,
}
impl<T: DeJson> DeJson for Group<T> {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(Group);
        parser_for!(Group);
        let mut fields = Fields::<T>::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            members: fields.members.ok_or_else(|| scope.err_nf("members"))?,
            leader: fields.leader.unwrap_or(None),
        })
    }
}

#[test]
fn manual_helpers_and_nested_wrappers() {
    let value = Group::<Person>::deserialize_json(
        r#"{"leader":{"name":"Ada","age":40},"members":[{"age":20,"name":"Bob"}]}"#,
    )
    .unwrap();
    assert_eq!(value.leader.unwrap().name, "Ada");
    assert_eq!(value.members[0].age, 20);
    assert!(Person::deserialize_json(r#"{"name":"","age":1}"#).is_err());
    assert!(Person::deserialize_json(r#"{"name":"A","age":1,"age":2}"#).is_err());
    assert!(Person::deserialize_json(r#"{"name":"A","age":1,"extra":2}"#).is_err());
}

#[test]
fn root_values_and_eof() {
    assert_eq!(u8::deserialize_json(" \n12\t").unwrap(), 12);
    assert_eq!(String::deserialize_json(r#""hello""#).unwrap(), "hello");
    assert!(bool::deserialize_json("true").unwrap());
    assert_eq!(Option::<Person>::deserialize_json("null").unwrap(), None);
    assert_eq!(Vec::<u8>::deserialize_json("[]").unwrap(), []);
    assert_eq!(
        Vec::<Vec<u8>>::deserialize_json("[[],[1],[]]").unwrap(),
        [vec![], vec![1], vec![]]
    );
    for input in ["", " \n", "12 13", "12,", "12]"] {
        assert!(u8::deserialize_json(input).is_err(), "{input:?}");
    }
    for input in ["[] []", "[],", "[]}", "[", "[1", "[[]", "[}", "[1}"] {
        assert!(Vec::<u8>::deserialize_json(input).is_err(), "{input:?}");
    }
    for input in [r#""unfinished"#, r#""bad\u00"#, "\"\0\"", "null false"] {
        assert!(String::deserialize_json(input).is_err(), "{input:?}");
    }
}

#[test]
fn nul_terminates_input_but_not_an_unfinished_container() {
    for input in ["12\0", "12\0ignored"] {
        assert_eq!(u8::deserialize_json(input).unwrap(), 12);
    }
    assert!(Vec::<u8>::deserialize_json("[]\0ignored")
        .unwrap()
        .is_empty());
    assert_eq!(Empty::deserialize_json("{}\0ignored").unwrap(), Empty {});
    for input in ["[\0]", "[1\0]"] {
        assert!(Vec::<u8>::deserialize_json(input).is_err(), "{input:?}");
    }
    assert!(Vec::<Vec<u8>>::deserialize_json("[[1]\0]").is_err());
    assert!(Empty::deserialize_json("{\0}").is_err());
}

#[derive(Debug, PartialEq, DeJson)]
struct Empty {}
#[derive(Debug, PartialEq, DeJson)]
struct Tree {
    empty: Empty,
    values: Vec<Vec<u8>>,
    children: Vec<Empty>,
}

#[test]
fn nested_scope_boundaries() {
    let input = r#"{"empty":{},"values":[[],[1,2],[]],"children":[{},{}]}"#;
    let value = Tree::deserialize_json(input).unwrap();
    assert_eq!(value.values, [vec![], vec![1, 2], vec![]]);
    assert_eq!(value.children.len(), 2);
    assert_eq!(Empty::deserialize_json("{} \n").unwrap(), Empty {});
    assert_eq!(
        Option::<Box<Empty>>::deserialize_json("{}").unwrap(),
        Some(Box::new(Empty {}))
    );
}

#[test]
fn strict_object_and_array_separators() {
    for input in [
        r#"{"name":"A","age":1,}"#,
        r#"{"name":"A",,"age":1}"#,
        r#"{,"name":"A","age":1}"#,
        r#"{"name":"A" "age":1}"#,
        r#"{"name":"A","age":}"#,
        r#"{"name" "A","age":1}"#,
        r#"{"name":"A","age":1]"#,
        r#"{"name":"A","age":1"#,
    ] {
        assert!(Person::deserialize_json(input).is_err(), "{input}");
    }
    for input in [
        "[1,]", "[1,,2]", "[,1]", "[1 2]", "[[] []]", "[[],]", "[[],[]",
    ] {
        assert!(Vec::<Vec<u8>>::deserialize_json(input).is_err(), "{input}");
        assert!(Vec::<u8>::deserialize_json(input).is_err(), "{input}");
    }
    for input in [
        r#"{"empty":{} "values":[],"children":[]}"#,
        r#"{"empty":{},"values":[],"children":[],}"#,
    ] {
        assert!(Tree::deserialize_json(input).is_err(), "{input}");
    }
}

#[test]
fn maps_and_fixed_sequences() {
    use std::collections::BTreeMap;
    #[cfg(feature = "std")]
    use std::collections::HashMap;
    let value = BTreeMap::<String, Vec<Empty>>::deserialize_json(r#"{"a":[{}],"b":[]}"#).unwrap();
    assert_eq!(value["a"].len(), 1);
    #[cfg(feature = "std")]
    assert_eq!(
        HashMap::<String, Empty>::deserialize_json(r#"{"a":{},"b":{}}"#)
            .unwrap()
            .len(),
        2
    );
    for input in [r#"{"a":{},}"#, r#"{"a":{} "b":{}}"#] {
        assert!(BTreeMap::<String, Empty>::deserialize_json(input).is_err());
        #[cfg(feature = "std")]
        assert!(HashMap::<String, Empty>::deserialize_json(input).is_err());
    }
    assert_eq!(<[u8; 0]>::deserialize_json("[]").unwrap(), []);
    assert_eq!(<[u8; 2]>::deserialize_json("[1,2]").unwrap(), [1, 2]);
    assert_eq!(
        <(Empty, Vec<u8>)>::deserialize_json("[{},[1]]").unwrap(),
        (Empty {}, vec![1])
    );
    for input in ["[]", "[1]", "[1,2,3]", "[1,2,]"] {
        assert!(<[u8; 2]>::deserialize_json(input).is_err());
    }
    for input in ["[{},]", "[{},[],[]]", "[{} []]"] {
        assert!(<(Empty, Vec<u8>)>::deserialize_json(input).is_err());
    }
}

#[test]
fn numeric_limits() {
    assert_eq!(u8::deserialize_json("255").unwrap(), 255);
    assert!(u8::deserialize_json("256").is_err());
    assert!(u8::deserialize_json("-1").is_err());
    assert_eq!(i8::deserialize_json("-128").unwrap(), -128);
    assert!(i8::deserialize_json("128").is_err());
    assert!(i8::deserialize_json("-129").is_err());
    assert!(i64::deserialize_json("18446744073709551615").is_err());
    assert!(i8::deserialize_json("18446744073709551615").is_err());
    assert_eq!(isize::deserialize_json("1").unwrap(), 1);
    assert_eq!(usize::deserialize_json("1").unwrap(), 1);
}

#[derive(Debug, DeJsonHelpers)]
struct Discard {
    keep: u8,
    ignored: u8,
}
impl DeJson for Discard {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(Discard);
        parser_for!(Discard);
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            match key {
                Key::ignored(_) => scope.skip_value()?,
                other => Parser::parse_field(other, scope)?,
            }
        }
        Ok(Self {
            keep: fields.keep.ok_or_else(|| scope.err_nf("keep"))?,
            ignored: 0,
        })
    }
}

#[test]
fn manual_discard_and_explicit_null_duplicates() {
    let value = Discard::deserialize_json(r#"{"ignored":{"nested":[{},[]]},"ignored":1,"keep":2}"#)
        .unwrap();
    assert_eq!((value.keep, value.ignored), (2, 0));
    assert!(
        Group::<Person>::deserialize_json(r#"{"members":[],"leader":null,"leader":null}"#).is_err()
    );
}

mod models {
    pub type Numbers = std::vec::Vec<u8>;
    #[derive(nanoserde::DeJsonHelpers)]
    pub struct Record {
        pub values: Numbers,
    }
}
mod readers {
    use super::models::{Numbers, Record};
    use nanoserde::{DeJson, DeJsonErr, DeJsonScope};
    impl DeJson for Record {
        fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
            nanoserde::fields_for!(super::models::Record);
            nanoserde::parser_for!(Record);
            let mut fields = Fields::new();
            while let Some(key) = fields.match_next_key(scope)? {
                Parser::parse_field(key, scope)?;
            }
            Ok(Self {
                values: fields.values.ok_or_else(|| scope.err_nf("values"))?,
            })
        }
    }
}
#[test]
fn crate_local_metadata_import() {
    assert_eq!(
        models::Record::deserialize_json(r#"{"values":[1,2]}"#)
            .unwrap()
            .values,
        [1, 2]
    );
}

#[test]
fn scope_completion_is_checked() {
    struct Incomplete;
    impl DeJson for Incomplete {
        fn de_json_value(_: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
            Ok(Self)
        }
    }
    assert!(Incomplete::deserialize_json("{}").is_err());
    assert!(Incomplete::deserialize_json("12").is_err());
}

#[derive(DeJsonHelpers, Debug)]
struct Fixed<const N: usize> {
    values: [u8; N],
}
impl<const N: usize> DeJson for Fixed<N> {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(Fixed);
        parser_for!(Fixed);
        let mut fields = Fields::<N>::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            values: fields.values.ok_or_else(|| scope.err_nf("values"))?,
        })
    }
}

#[derive(Debug)]
struct Marker<'a>(std::marker::PhantomData<&'a ()>);
impl DeJson for Marker<'_> {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        scope.parse_null()?;
        Ok(Self(std::marker::PhantomData))
    }
}
#[derive(DeJsonHelpers)]
struct LifetimeFields<'a> {
    marker: Marker<'a>,
}
impl<'a> DeJson for LifetimeFields<'a> {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(LifetimeFields);
        parser_for!(LifetimeFields);
        let mut fields = Fields::<'a>::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            marker: fields.marker.ok_or_else(|| scope.err_nf("marker"))?,
        })
    }
}

#[derive(DeJson, Debug)]
struct WireNumber {
    n: u8,
}
struct Number(u8);
impl From<&WireNumber> for Number {
    fn from(value: &WireNumber) -> Self {
        Self(value.n)
    }
}
#[derive(DeJsonHelpers)]
struct Attributes {
    #[nserde(rename = "wire")]
    value: u8,
    #[nserde(proxy = "WireNumber")]
    number: Number,
    #[nserde(proxy = "WireNumber")]
    maybe: Option<Number>,
    #[nserde(skip)]
    ignored: std::marker::PhantomData<()>,
}
impl DeJson for Attributes {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(Attributes);
        parser_for!(Attributes);
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            value: fields.value.ok_or_else(|| scope.err_nf("wire"))?,
            number: fields.number.ok_or_else(|| scope.err_nf("number"))?,
            maybe: fields.maybe.unwrap_or(None),
            ignored: Default::default(),
        })
    }
}

#[test]
fn metadata_parameters_and_attributes() {
    assert_eq!(
        Fixed::<2>::deserialize_json(r#"{"values":[1,2]}"#)
            .unwrap()
            .values,
        [1, 2]
    );
    assert_eq!(
        Fixed::<0>::deserialize_json(r#"{"values":[]}"#)
            .unwrap()
            .values,
        []
    );
    let value = LifetimeFields::deserialize_json(r#"{"marker":null}"#).unwrap();
    let _ = value.marker;
    let value = Attributes::deserialize_json(
        r#"{"wire":1,"number":{"n":2},"maybe":{"n":3},"ignored":[{}]}"#,
    )
    .unwrap();
    assert_eq!(
        (value.value, value.number.0, value.maybe.unwrap().0),
        (1, 2, 3)
    );
}

#[test]
fn fixed_array_drops_values_on_errors() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LIVE: AtomicUsize = AtomicUsize::new(0);
    struct Counted;
    impl Drop for Counted {
        fn drop(&mut self) {
            LIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl DeJson for Counted {
        fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
            scope.parse_null()?;
            LIVE.fetch_add(1, Ordering::SeqCst);
            Ok(Self)
        }
    }
    for input in [
        "[null",
        "[null,]",
        "[null null]",
        "[null,1]",
        "[null,null,null]",
        "[null,null,]",
    ] {
        assert!(<[Counted; 2]>::deserialize_json(input).is_err(), "{input}");
        assert_eq!(LIVE.load(Ordering::SeqCst), 0, "{input}");
    }
    let values = <[Counted; 2]>::deserialize_json("[null,null]").unwrap();
    assert_eq!(LIVE.load(Ordering::SeqCst), 2);
    drop(values);
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "std")]
#[test]
fn validation_error_preserves_custom_error() {
    #[derive(Debug)]
    struct Invalid;
    impl std::fmt::Display for Invalid {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("invalid")
        }
    }
    impl std::error::Error for Invalid {}
    struct Validated;
    impl DeJson for Validated {
        fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
            scope.parse_u8()?;
            Err(scope.err_user(Invalid))
        }
    }
    let error = Vec::<Validated>::deserialize_json("[1]").err().unwrap();
    match error.msg {
        nanoserde::DeJsonErrReason::UserError(inner) => {
            assert!(inner.downcast_ref::<Invalid>().is_some())
        }
        other => panic!("{other:?}"),
    }
}

#[derive(DeJsonHelpers)]
struct Recursive {
    next: Option<Box<Self>>,
}
impl DeJson for Recursive {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(Recursive);
        parser_for!(Recursive);
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            next: fields.next.unwrap_or(None),
        })
    }
}
#[derive(DeJsonHelpers)]
struct NoFields {}
impl DeJson for NoFields {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        fields_for!(NoFields);
        parser_for!(NoFields);
        let mut fields = Fields::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {})
    }
}
#[test]
fn recursive_and_empty_helpers() {
    assert!(Recursive::deserialize_json(r#"{"next":{"next":null}}"#)
        .unwrap()
        .next
        .unwrap()
        .next
        .is_none());
    assert!(NoFields::deserialize_json("{}").is_ok());
    assert!(NoFields::deserialize_json(r#"{"extra":1}"#).is_err());
}

#[derive(nanoserde::SerJson, DeJson, Debug, PartialEq)]
enum ArrayVariant {
    Values(Vec<u8>),
    Pair((u8, u8)),
}
#[test]
fn single_field_array_variants() {
    for value in [ArrayVariant::Values(vec![1, 2]), ArrayVariant::Pair((3, 4))] {
        let json = nanoserde::SerJson::serialize_json(&value);
        assert_eq!(ArrayVariant::deserialize_json(&json).unwrap(), value);
    }
}

#[derive(DeJsonHelpers)]
#[nserde(crate = "codec")]
struct RenamedLibrary<T>
where
    T: DeJson,
{
    value: T,
}
impl<T: DeJson> DeJson for RenamedLibrary<T> {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        use nanoserde as codec;
        fields_for!(RenamedLibrary);
        parser_for!(RenamedLibrary);
        let mut fields = Fields::<T>::new();
        while let Some(key) = fields.match_next_key(scope)? {
            Parser::parse_field(key, scope)?;
        }
        Ok(Self {
            value: fields.value.ok_or_else(|| scope.err_nf("value"))?,
        })
    }
}
#[test]
fn helpers_with_where_clause_and_renamed_library() {
    assert_eq!(
        RenamedLibrary::<u8>::deserialize_json(r#"{"value":2}"#)
            .unwrap()
            .value,
        2
    );
}

#[test]
fn extra_scopes_allow_delegation_without_overrides() {
    struct Delegated<T>(T);
    impl<T: DeJson> DeJson for Delegated<T> {
        fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
            T::de_json(scope).map(Self)
        }
    }
    let value = Delegated::<Delegated<Tree>>::deserialize_json(
        r#"{"empty":{},"values":[[],[1,2]],"children":[{}]}"#,
    )
    .unwrap();
    assert_eq!(value.0 .0.values, [vec![], vec![1, 2]]);
    assert!(Delegated::<Delegated<Empty>>::deserialize_json("{}").is_ok());
    assert_eq!(
        Delegated::<Delegated<Vec<u8>>>::deserialize_json("[]")
            .unwrap()
            .0
             .0,
        []
    );
    assert_eq!(
        Delegated::<Delegated<u8>>::deserialize_json("12")
            .unwrap()
            .0
             .0,
        12
    );
    assert!(Delegated::<Vec<u8>>::deserialize_json("[1,]").is_err());
    assert!(Delegated::<Empty>::deserialize_json("{} {}").is_err());
}

#[test]
fn scope_open_checks_do_not_consume_mismatched_input() {
    use nanoserde::{DeJsonState, DeJsonTok};
    let mut state = DeJsonState::new("[]");
    let mut root = DeJsonScope::root(&mut state);
    root.next_tok().unwrap();
    let mut scope = root.new_scope().unwrap();
    assert!(scope.obj_open().is_err());
    assert_eq!(scope.token(), &DeJsonTok::BlockOpen);
    // A failed opener check hasn't claimed the scope or consumed input.
    assert!(Vec::<u8>::de_json(&mut scope).unwrap().is_empty());
    scope.arr_close().unwrap();

    let mut state = DeJsonState::new("{}");
    let mut root = DeJsonScope::root(&mut state);
    root.next_tok().unwrap();
    let mut scope = root.new_scope().unwrap();
    assert!(scope.arr_open().is_err());
    assert_eq!(scope.token(), &DeJsonTok::CurlyOpen);
    Empty::de_json(&mut scope).unwrap();
    scope.obj_close().unwrap();
}

#[test]
fn claimed_scopes_must_reach_their_own_closer() {
    use nanoserde::DeJsonState;
    // Claiming the object disables the unused-scope completion exception.
    let mut state = DeJsonState::new("{}");
    let mut root = DeJsonScope::root(&mut state);
    root.next_tok().unwrap();
    let mut scope = root.new_scope().unwrap();
    scope.obj_open().unwrap();
    Empty::de_json(&mut scope).unwrap();
    assert!(scope.obj_close().is_err());

    // Advancing without an explicit open call also claims the scope. A nested
    // child's closer must not be mistaken for the enclosing array's closer.
    let mut state = DeJsonState::new("[[]]");
    let mut root = DeJsonScope::root(&mut state);
    root.next_tok().unwrap();
    let mut scope = root.new_scope().unwrap();
    scope.next_tok().unwrap();
    Vec::<u8>::de_json(&mut scope).unwrap();
    assert!(scope.arr_close().is_err());
    assert!(!scope.array_next_val().unwrap());
    scope.arr_close().unwrap();
    assert!(scope.next_tok().is_err());
}
