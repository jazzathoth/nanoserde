#![cfg(feature = "json")]

use nanoserde::{
    DeJson, JsonKey, SerJson, SerJsonErr, SerJsonErrReason, SerJsonScope, SerJsonState,
};
use std::collections::BTreeMap;

#[test]
fn nested_containers_and_empty_arrays() {
    #[derive(SerJson)]
    struct Empty {}
    #[derive(SerJson)]
    struct Record {
        objects: Vec<Empty>,
        arrays: Vec<Vec<u8>>,
        pair: (Empty, Vec<u8>),
    }
    let record = Record {
        objects: vec![Empty {}, Empty {}],
        arrays: vec![vec![], vec![1, 2], vec![]],
        pair: (Empty {}, vec![]),
    };
    assert_eq!(
        record.serialize_json().unwrap(),
        r#"{"objects":[{},{}],"arrays":[[],[1,2],[]],"pair":[{},[]]}"#
    );
    assert_eq!([0u8; 0].serialize_json().unwrap(), "[]");
    assert_eq!((&[] as &[u8]).serialize_json().unwrap(), "[]");
}

#[test]
fn omitted_fields_do_not_leave_separators() {
    #[derive(SerJson)]
    #[nserde(skip_on_none)]
    struct Record {
        first: Option<u8>,
        middle: u8,
        last: Option<u8>,
        #[nserde(skip)]
        ignored: u8,
        #[nserde(serialize_none_as_null)]
        explicit: Option<u8>,
    }
    let record = Record {
        first: None,
        middle: 1,
        last: None,
        ignored: 2,
        explicit: None,
    };
    let _ = record.ignored;
    assert_eq!(
        record.serialize_json().unwrap(),
        r#"{"middle":1,"explicit":null}"#
    );
    assert_eq!(Option::<u8>::None.serialize_json().unwrap(), "null");

    #[derive(SerJson)]
    enum RecordVariant {
        Fields {
            #[nserde(skip_on_none)]
            first: Option<u8>,
            middle: u8,
            #[nserde(skip_on_none)]
            last: Option<u8>,
        },
        Empty {
            #[nserde(skip_on_none)]
            absent: Option<u8>,
        },
    }
    assert_eq!(
        RecordVariant::Fields {
            first: None,
            middle: 1,
            last: None
        }
        .serialize_json()
        .unwrap(),
        r#"{"Fields":{"middle":1}}"#
    );
    assert_eq!(
        RecordVariant::Empty { absent: None }
            .serialize_json()
            .unwrap(),
        r#"{"Empty":{}}"#
    );
}

#[test]
fn keys_use_string_escaping() {
    let key = "quote\" slash\\ line\n tab\t nul\0";
    let map = BTreeMap::from([(key.to_owned(), 12u8)]);
    let json = map.serialize_json().unwrap();
    assert_eq!(
        BTreeMap::<String, u8>::deserialize_json(&json).unwrap(),
        map
    );
    assert_eq!(json, r#"{"quote\" slash\\ line\n tab\t nul\u0000":12}"#);

    #[derive(SerJson)]
    struct Renamed {
        #[nserde(rename = "display name")]
        name: String,
    }
    assert_eq!(
        Renamed { name: "Ada".into() }.serialize_json().unwrap(),
        r#"{"display name":"Ada"}"#
    );
}

#[test]
fn numeric_keys_serialize_as_strings_without_automatic_conversion() {
    let map = BTreeMap::from([(-12i16, vec![1u8, 2]), (30, vec![])]);
    let json = map.serialize_json().unwrap();
    assert_eq!(json, r#"{"-12":[1,2],"30":[]}"#);
    assert_eq!(
        BTreeMap::<String, Vec<u8>>::deserialize_json(&json).unwrap(),
        BTreeMap::from([("-12".to_owned(), vec![1u8, 2]), ("30".to_owned(), vec![])])
    );
    assert!(BTreeMap::<i16, Vec<u8>>::deserialize_json(&json).is_err());
    assert!(BTreeMap::<u8, u8>::deserialize_json(r#"{"12":1}"#).is_err());
    assert!(BTreeMap::<u8, u8>::deserialize_json(r#"{"256":1}"#).is_err());
    assert!(BTreeMap::<u8, u8>::deserialize_json(r#"{"-1":1}"#).is_err());
    assert!(u8::deserialize_json(r#""12""#).is_err());
    #[cfg(feature = "std")]
    {
        let map = std::collections::HashMap::from([(12u32, "value".to_owned())]);
        let json = map.serialize_json().unwrap();
        assert_eq!(json, r#"{"12":"value"}"#);
        assert_eq!(
            std::collections::HashMap::<String, String>::deserialize_json(&json).unwrap(),
            std::collections::HashMap::from([("12".to_owned(), "value".to_owned())])
        );
    }
}

#[test]
fn non_finite_float_values_and_keys() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(value.serialize_json().unwrap(), "null");
        let mut state = SerJsonState::new(String::new());
        let mut root = SerJsonScope::root(&mut state);
        let mut root = root.new_scope().unwrap();
        root.obj_open().unwrap();
        assert!(matches!(
            root.write_field(&value, &1u8).unwrap_err().msg,
            SerJsonErrReason::Serialization(_)
        ));
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(value.serialize_json().unwrap(), "null");
        let mut state = SerJsonState::new(String::new());
        assert!(value
            .ser_json_key(&mut SerJsonScope::root(&mut state))
            .is_err());
    }
    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    let mut root = root.new_scope().unwrap();
    root.obj_open().unwrap();
    root.write_field(&1.5, &true).unwrap();
    root.obj_close().unwrap();
    assert_eq!(state.out, r#"{"1.5":true}"#);
}

#[test]
fn manual_keys_values_and_depth() -> Result<(), SerJsonErr> {
    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    assert_eq!(root.depth(), 0);
    {
        let mut object = root.new_scope()?;
        object.obj_open()?;
        assert_eq!(object.depth(), 1);
        object.write_key("values")?;
        {
            let mut array = object.new_scope()?;
            assert_eq!(array.depth(), 2);
            array.arr_open()?;
            array.begin_element()?;
            array.write_value(&1u8)?;
            array.write_element(&Vec::<u8>::new())?;
            array.arr_close()?;
        }
        object.write_field("other", &None::<bool>)?;
        object.obj_close()?;
    }
    assert_eq!(state.out, r#"{"values":[1,[]],"other":null}"#);
    Ok(())
}

#[test]
fn invalid_block_and_entry_operations() {
    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    let mut scope = root.new_scope().unwrap();
    assert!(matches!(
        scope.obj_close().unwrap_err().msg,
        SerJsonErrReason::Block(_)
    ));
    assert!(matches!(
        scope.write_field("x", &1u8).unwrap_err().msg,
        SerJsonErrReason::Block(_)
    ));
    scope.arr_open().unwrap();
    assert!(matches!(
        scope.obj_open().unwrap_err().msg,
        SerJsonErrReason::Block(_)
    ));
    assert!(matches!(
        scope.write_field("x", &1u8).unwrap_err().msg,
        SerJsonErrReason::UnexpectedEntry(_)
    ));
    assert!(matches!(
        scope.obj_close().unwrap_err().msg,
        SerJsonErrReason::Block(_)
    ));
    scope.arr_close().unwrap();
    assert!(matches!(
        scope.write_element(&1u8).unwrap_err().msg,
        SerJsonErrReason::Block(_)
    ));
    assert!(matches!(
        scope.arr_close().unwrap_err().msg,
        SerJsonErrReason::Block(_)
    ));
}

#[test]
fn incomplete_manual_entries() {
    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    let mut scope = root.new_scope().unwrap();
    scope.obj_open().unwrap();
    assert!(scope.write_element(&1u8).is_err());
    scope.write_key("missing").unwrap();
    assert!(matches!(
        scope.obj_close().unwrap_err().msg,
        SerJsonErrReason::Incomplete(_)
    ));
    assert!(matches!(
        scope.write_key("another").unwrap_err().msg,
        SerJsonErrReason::Incomplete(_)
    ));
    scope.out.push_str(" \n\t");
    scope.write_value(&true).unwrap();
    assert!(matches!(
        scope.write_value(&false).unwrap_err().msg,
        SerJsonErrReason::Incomplete(_)
    ));
    scope.obj_close().unwrap();

    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    let mut scope = root.new_scope().unwrap();
    scope.obj_open().unwrap();
    scope.write_string("no colon");
    assert!(matches!(
        scope.write_value(&1u8).unwrap_err().msg,
        SerJsonErrReason::Incomplete(_)
    ));

    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    let mut scope = root.new_scope().unwrap();
    scope.arr_open().unwrap();
    scope.begin_element().unwrap();
    scope.write_value(&1u8).unwrap();
    scope.out.push(',');
    assert!(scope.arr_close().is_err());
}

#[test]
fn errors_propagate_through_containers_and_derive() {
    struct Fails;
    impl SerJson for Fails {
        fn ser_json(&self, scope: &mut SerJsonScope<'_>) -> Result<(), SerJsonErr> {
            Err(scope.err_serialization("custom failure"))
        }
    }
    #[derive(SerJson)]
    struct Record {
        nested: Option<Box<Vec<Fails>>>,
    }
    let error = Record {
        nested: Some(Box::new(vec![Fails])),
    }
    .serialize_json()
    .unwrap_err();
    assert!(
        matches!(error.msg, SerJsonErrReason::Serialization(ref message) if message == "custom failure")
    );
}

#[cfg(feature = "std")]
#[test]
fn user_error_preserves_original_error() {
    #[derive(Debug)]
    struct CustomError;
    impl std::fmt::Display for CustomError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("custom")
        }
    }
    impl std::error::Error for CustomError {}
    struct Fails;
    impl SerJson for Fails {
        fn ser_json(&self, scope: &mut SerJsonScope<'_>) -> Result<(), SerJsonErr> {
            Err(scope.err_user(CustomError))
        }
    }
    let error = vec![Fails].serialize_json().unwrap_err();
    match &error.msg {
        SerJsonErrReason::UserError(inner) => {
            assert!(inner.downcast_ref::<CustomError>().is_some())
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn named_variant_annotations_and_proxies() {
    struct Domain(u8);
    #[derive(SerJson)]
    struct Proxy {
        value: u8,
    }
    impl From<&Domain> for Proxy {
        fn from(value: &Domain) -> Self {
            Self { value: value.0 }
        }
    }
    #[derive(SerJson)]
    enum Variant {
        Fields {
            #[nserde(proxy = "Proxy", rename = "value_out")]
            value: Domain,
            #[nserde(proxy = "Proxy")]
            optional: Option<Domain>,
            #[nserde(skip)]
            ignored: Domain,
        },
    }
    let value = Variant::Fields {
        value: Domain(12),
        optional: Some(Domain(13)),
        ignored: Domain(99),
    };
    let Variant::Fields { ignored, .. } = &value;
    assert_eq!(ignored.0, 99);
    assert_eq!(
        value.serialize_json().unwrap(),
        r#"{"Fields":{"value_out":{"value":12},"optional":{"value":13}}}"#
    );
}

#[test]
fn indent_controls_scope_lifetime() -> Result<(), SerJsonErr> {
    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    {
        let mut object = root.new_scope()?;
        assert!(object.write_field("before_open", &1u8).is_err());
        assert!(object.new_scope().is_err());
        object.obj_open()?;
        object.write_key("child")?;
        {
            let mut array = object.new_scope()?;
            array.arr_open()?;
            array.write_element(&1u8)?;
            array.arr_close()?;
            assert!(array.write_element(&2u8).is_err());
            assert!(array.new_scope().is_err());
            assert!(array.arr_close().is_err());
            assert!(array.arr_open().is_err());
        }
        object.write_field("after_child", &true)?;
        object.obj_close()?;
        assert!(object.write_field("after_close", &false).is_err());
        assert!(object.new_scope().is_err());
    }
    assert_eq!(state.out, r#"{"child":[1],"after_child":true}"#);
    Ok(())
}

#[test]
fn an_unclosed_child_prevents_parent_writes() -> Result<(), SerJsonErr> {
    let mut state = SerJsonState::new(String::new());
    let mut root = SerJsonScope::root(&mut state);
    let mut object = root.new_scope()?;
    object.obj_open()?;
    object.write_key("child")?;
    {
        let mut array = object.new_scope()?;
        array.arr_open()?;
        array.write_element(&1u8)?;
        // Dropping a scope does not write its closer or restore the indent.
    }
    assert!(object.write_field("next", &2u8).is_err());
    assert!(object.write_value(&2u8).is_err());
    assert!(object.new_scope().is_err());
    assert!(object.obj_close().is_err());
    Ok(())
}

#[test]
fn option_null_default_and_explicit_omission() {
    #[derive(SerJson)]
    struct Record {
        normal: Option<u8>,
        #[nserde(skip_on_none)]
        omitted: Option<u8>,
        #[nserde(default = "12")]
        defaulted: Option<u8>,
    }
    assert_eq!(
        Record {
            normal: None,
            omitted: None,
            defaulted: None
        }
        .serialize_json()
        .unwrap(),
        r#"{"normal":null,"defaulted":null}"#
    );
    assert_eq!(
        Record {
            normal: Some(1),
            omitted: Some(2),
            defaulted: Some(3)
        }
        .serialize_json()
        .unwrap(),
        r#"{"normal":1,"omitted":2,"defaulted":3}"#
    );

    #[derive(SerJson)]
    enum Variant {
        Fields { value: Option<u8> },
    }
    assert_eq!(
        Variant::Fields { value: None }.serialize_json().unwrap(),
        r#"{"Fields":{"value":null}}"#
    );

    struct Domain;
    #[derive(SerJson)]
    struct Proxy {
        value: u8,
    }
    impl From<&Domain> for Proxy {
        fn from(_: &Domain) -> Self {
            Proxy { value: 1 }
        }
    }
    #[derive(SerJson)]
    struct WithProxy {
        #[nserde(proxy = "Proxy")]
        normal: Option<Domain>,
        #[nserde(proxy = "Proxy", skip_on_none)]
        omitted: Option<Domain>,
    }
    assert_eq!(
        WithProxy {
            normal: None,
            omitted: None
        }
        .serialize_json()
        .unwrap(),
        r#"{"normal":null}"#
    );
}
