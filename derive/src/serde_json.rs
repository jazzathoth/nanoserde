use ::alloc::format;
use ::alloc::string::{String, ToString};
use ::alloc::{vec, vec::Vec};

use crate::parse::Category;
use crate::shared::{enum_bounds_strings, struct_bounds_strings};
use crate::{
    parse::{Enum, Field, Struct},
    shared,
};

use proc_macro::TokenStream;

pub fn derive_ser_json_proxy(proxy_type: &str, type_: &str, crate_name: &str) -> TokenStream {
    format!(
        "impl {crate_name}::SerJson for {type_} {{
            fn ser_json(&self, s: &mut {crate_name}::SerJsonScope<'_>) -> Result<(), {crate_name}::SerJsonErr> {{
                let proxy: {proxy_type} = self.into();
                proxy.ser_json(s)
            }}
        }}"
    ).parse().unwrap()
}

fn ser_proxy_guard(fieldname: &str, field: &Field) -> String {
    if let Some(proxy) = crate::shared::attrs_proxy(&field.attributes) {
        if field.ty.base() == "Option" {
            format!(
                "&{{({fieldname}).as_ref().map(|f| {{let proxy: {proxy} = Into::into(f);proxy}})}}"
            )
        } else {
            format!("&{{let proxy: {proxy} = Into::into({fieldname});proxy}}",)
        }
    } else {
        fieldname.to_string()
    }
}

// Generate one field write; omission happens before the scope writes its separator.
fn ser_json_field(field: &Field, access: &str, struct_skip_on_none: bool) -> String {
    if shared::attrs_skip(&field.attributes) {
        return String::new();
    }
    let struct_field = field.field_name.as_ref().unwrap();
    let json_field =
        shared::attrs_rename(&field.attributes).unwrap_or_else(|| struct_field.clone());
    let value = ser_proxy_guard(access, field);
    let skip_on_none = shared::attrs_skip_on_none(&field.attributes)
        || (struct_skip_on_none && !shared::attrs_serialize_none_as_null(&field.attributes));
    if field.ty.base() == "Option" && skip_on_none {
        format!("if let Some(value) = {value} {{ s.write_field({json_field:?}, value)?; }}")
    } else {
        format!("s.write_field({json_field:?}, {value})?;")
    }
}

pub fn derive_ser_json_struct(struct_: &Struct, crate_name: &str) -> TokenStream {
    let (bounds, args) = struct_bounds_strings(struct_, "SerJson", crate_name);
    let mut body = String::new();
    for field in &struct_.fields {
        let struct_field = field.field_name.as_ref().unwrap();
        body.push_str(&ser_json_field(
            field,
            &format!("&self.{struct_field}"),
            shared::attrs_skip_on_none(&struct_.attributes),
        ));
    }
    let name = struct_
        .name
        .as_ref()
        .expect("Cannot implement for anonymous struct");
    format!("impl{bounds} {crate_name}::SerJson for {name}{args} {{
        fn ser_json(&self, parent: &mut {crate_name}::SerJsonScope<'_>) -> Result<(), {crate_name}::SerJsonErr> {{
            let mut s = parent.new_scope()?;
            s.obj_open()?;
            {body}
            s.obj_close()
        }}
    }}").parse().unwrap()
}

pub fn derive_de_json_named(
    name: &str,
    defaults: bool,
    fields: &[Field],
    crate_name: &str,
) -> TokenStream {
    let mut local_vars = Vec::new();
    let mut struct_field_names = Vec::new();
    let mut matches = Vec::new();
    let mut unwraps = Vec::new();
    let mut skipped = Vec::new();

    let container_attr_default = defaults;

    for field in fields {
        let struct_fieldname = field.field_name.as_ref().unwrap().to_string();
        let localvar = format!("_{}", struct_fieldname);
        let field_attr_default = shared::attrs_default(&field.attributes);
        let field_attr_default_with = shared::attrs_default_with(&field.attributes);
        let default_val = if let Some(v) = field_attr_default {
            if let Some(mut val) = v {
                if field.ty.base() == "String"
                    || field
                        .ty
                        .wraps
                        .as_ref()
                        .is_some_and(|wrapped| wrapped.iter().any(|ty| ty.base() == "String"))
                {
                    val = format!("\"{}\".to_string()", val)
                }
                if field.ty.base() == "Option" {
                    val = format!("Some({})", val);
                }
                Some(val)
            } else if field.ty.base() != "Option" {
                Some(String::from("Default::default()"))
            } else {
                Some(String::from("None"))
            }
        } else if let Some(mut v) = field_attr_default_with {
            v.push_str("()");
            Some(v)
        } else {
            None
        };
        let json_fieldname =
            shared::attrs_rename(&field.attributes).unwrap_or(struct_fieldname.clone());
        let proxy = crate::shared::attrs_proxy(&field.attributes);
        let skip = crate::shared::attrs_skip(&field.attributes);

        let proxified_t = if let Some(proxy) = proxy {
            if field.ty.base() == "Option" {
                format!("Some(From::<&{proxy}>::from(&t))")
            } else {
                format!("From::<&{proxy}>::from(&t)")
            }
        } else {
            "t".to_string()
        };

        if !skip {
            if field.ty.base() == "Option" || container_attr_default || default_val.is_some() {
                if let Some(default_val) = default_val {
                    unwraps.push(format!(
                        "{}.map_or_else(|| {}, |t| {})",
                        localvar, default_val, proxified_t,
                    ));
                } else {
                    unwraps.push(format!(
                        "{}.map(|t| {}).unwrap_or_default()",
                        localvar, proxified_t,
                    ));
                }
            } else {
                unwraps.push(format!(
                    "{}.map(|t| {}).ok_or_else(|| s.err_nf(\"{}\"))?",
                    localvar, proxified_t, struct_fieldname
                ));
            }
            matches.push((json_fieldname, localvar.clone()));
            local_vars.push(localvar);
        } else {
            skipped.push(json_fieldname);
            unwraps.push(default_val.unwrap_or_else(|| String::from("Default::default()")));
        }

        struct_field_names.push(struct_fieldname);
    }

    let mut r = String::new();
    for local_var in &local_vars {
        l!(r, "let mut {} = None;", local_var);
    }
    let names = matches
        .iter()
        .map(|(name, _)| format!("{name:?}"))
        .chain(skipped.iter().map(|name| format!("{name:?}")))
        .collect::<Vec<_>>()
        .join(",");
    l!(r, "s.obj_open()?;");
    l!(r, "while let Some(key) = s.next_key(&[{}])? {{", names);
    l!(r, "match key {");
    for (json_field_name, local_var) in matches.iter() {
        l!(r, "{json_field_name:?} => {{ if {local_var}.is_some() {{ return Err(s.err_parse(concat!(\"duplicate field \", {json_field_name:?}))); }} {local_var} = Some({crate_name}::DeJson::de_json(s)?); }},",);
    }
    for name in skipped {
        l!(r, "{name:?} => s.skip_value()?,",);
    }
    l!(r, "_ => unreachable!(),");
    l!(r, "}");
    l!(r, "}");
    l!(r, "s.obj_close()?;");
    l!(r, "{} {{", name);
    for (field_name, unwrap) in struct_field_names.iter().zip(unwraps.iter()) {
        l!(r, "{}: {},", field_name, unwrap);
    }
    l!(r, "}");

    r.parse().unwrap()
}

pub fn derive_de_json_proxy(proxy_type: &str, type_: &str, crate_name: &str) -> TokenStream {
    format!(
        "impl {}::DeJson for {} {{
            #[allow(clippy::ignored_unit_patterns)]
            fn de_json_value(s: &mut {}::DeJsonScope<'_, '_>) -> ::core::result::Result<Self, {}::DeJsonErr> {{
                let proxy: {} = {}::DeJson::de_json(s)?;
                ::core::result::Result::Ok(Into::into(&proxy))
            }}
        }}",
        crate_name, type_, crate_name, crate_name, proxy_type, crate_name
    )
    .parse()
    .unwrap()
}

pub fn derive_de_json_struct(struct_: &Struct, crate_name: &str) -> TokenStream {
    let body = derive_de_json_named(
        struct_
            .name
            .as_ref()
            .expect("Cannot implement for anonymous struct"),
        shared::attrs_default(&struct_.attributes).is_some()
            || shared::attrs_default_with(&struct_.attributes).is_some(),
        &struct_.fields[..],
        crate_name,
    );
    let (generic_w_bounds, generic_no_bounds) =
        struct_bounds_strings(struct_, "DeJson", crate_name);

    format!(
        "impl{} {}::DeJson for {}{} {{
            #[allow(clippy::ignored_unit_patterns)]
            fn de_json_value(s: &mut {}::DeJsonScope<'_, '_>) -> ::core::result::Result<Self,
            {}::DeJsonErr> {{
                ::core::result::Result::Ok({{ {} }})
            }}
        }}",
        generic_w_bounds,
        crate_name,
        struct_
            .name
            .as_ref()
            .expect("Cannot implement for anonymous struct"),
        generic_no_bounds,
        crate_name,
        crate_name,
        body
    )
    .parse()
    .unwrap()
}

pub fn derive_ser_json_enum(enum_: &Enum, crate_name: &str) -> TokenStream {
    let (bounds, args) = enum_bounds_strings(enum_, "SerJson", crate_name);
    let mut arms = String::new();
    for variant in &enum_.variants {
        let field_name = variant.field_name.as_ref().unwrap();
        let json_field =
            shared::attrs_rename(&variant.attributes).unwrap_or_else(|| field_name.clone());
        match &variant.ty.ident {
            Category::None => {
                l!(arms, "Self::{field_name} => s.write_value({json_field:?}),",);
            }
            Category::AnonymousStruct { contents } => {
                let mut body = String::new();
                let mut names = Vec::new();
                for field in &contents.fields {
                    let name = field.field_name.as_ref().unwrap();
                    if shared::attrs_skip(&field.attributes) {
                        names.push(format!("{name}: _"));
                    } else {
                        names.push(name.clone());
                    }
                    body.push_str(&ser_json_field(
                        field,
                        name,
                        shared::attrs_skip_on_none(&enum_.attributes),
                    ));
                }
                let names = names.join(",");
                l!(
                    arms,
                    "Self::{field_name} {{ {names} }} => {{
                    let mut outer = s.new_scope()?; outer.obj_open()?;
                    outer.write_key({json_field:?})?;
                    {{ let mut s = outer.new_scope()?; s.obj_open()?; {body} s.obj_close()?; }}
                    outer.obj_close()
                }},",
                );
            }
            Category::Tuple { contents } => {
                let names: Vec<_> = (0..contents.len()).map(|i| format!("f{i}")).collect();
                let pattern = names.join(",");
                let mut body = String::new();
                if contents.len() == 1 {
                    l!(body, "outer.write_field({json_field:?}, {})?;", names[0]);
                } else {
                    l!(body, "outer.write_key({json_field:?})?; {{ let mut array = outer.new_scope()?; array.arr_open()?;",);
                    for name in &names {
                        l!(body, "array.write_element({name})?;",);
                    }
                    l!(body, "array.arr_close()?; }}",);
                }
                l!(
                    arms,
                    "Self::{field_name}({pattern}) => {{
                    let mut outer = s.new_scope()?; outer.obj_open()?;
                    {body}
                    outer.obj_close()
                }},",
                );
            }
            other => unimplemented!("Unexpected type in enum: {:?}", other),
        }
    }
    let name = &enum_.name;
    format!("impl{bounds} {crate_name}::SerJson for {name}{args} {{
        fn ser_json(&self, s: &mut {crate_name}::SerJsonScope<'_>) -> Result<(), {crate_name}::SerJsonErr> {{
            match self {{ {arms} }}
        }}
    }}").parse().unwrap()
}

pub fn derive_de_json_enum(enum_: &Enum, crate_name: &str) -> TokenStream {
    let (bounds, args) = enum_bounds_strings(enum_, "DeJson", crate_name);
    let mut units = String::new();
    let mut objects = String::new();
    let mut names = Vec::new();
    for variant in &enum_.variants {
        let field_name = variant.field_name.as_ref().unwrap();
        let wire = shared::attrs_rename(&variant.attributes).unwrap_or(field_name.clone());
        match &variant.ty.ident {
            Category::None => {
                l!(units, "{wire:?} => Self::{field_name},",);
            }
            Category::AnonymousStruct { contents } => {
                names.push(format!("{wire:?}"));
                let body = derive_de_json_named(
                    &format!("Self::{field_name}"),
                    false,
                    &contents.fields,
                    crate_name,
                );
                l!(objects, "{wire:?} => {{ let mut nested = s.new_scope()?; let s = &mut nested; {body} }},",);
            }
            Category::Tuple { contents } => {
                names.push(format!("{wire:?}"));
                if contents.len() == 1 {
                    l!(
                        objects,
                        "{wire:?} => Self::{field_name}(s.parse_variant_value()?),",
                    );
                } else {
                    let mut reads = String::new();
                    for _ in contents {
                        l!(reads, "{{ if !s.array_next_val()? {{ return Err(s.err_token(\"array element\")); }} {crate_name}::DeJson::de_json(s)? }},",);
                    }
                    l!(objects, "{wire:?} => {{ let mut nested = s.new_scope()?; let s = &mut nested; s.arr_open()?; let value = Self::{field_name}({reads}); if s.array_next_val()? {{ return Err(s.err_token(\"]\")); }} s.arr_close()?; value }},",);
                }
            }
            _ => unreachable!(),
        }
    }
    let names = names.join(",");
    let unit_read = if units.is_empty() {
        String::new()
    } else {
        format!("if s.is_string() {{ let name = s.parse_string()?; return Ok(match name.as_str() {{ {units} _ => return Err(s.err_enum(&name)), }}); }}")
    };
    let object_read = if objects.is_empty() {
        "Err(s.err_token(\"enum string\"))".to_string()
    } else {
        format!("s.obj_open()?; let name = s.next_key(&[{names}])?.ok_or_else(|| s.err_token(\"enum variant\"))?; let value = match name {{ {objects} _ => unreachable!(), }}; if s.object_next_entry()? {{ return Err(s.err_token(\"}}\")); }} s.obj_close()?; Ok(value)")
    };
    format!("impl{bounds} {crate_name}::DeJson for {name}{args} {{
        fn de_json_value(s: &mut {crate_name}::DeJsonScope<'_, '_>) -> ::core::result::Result<Self, {crate_name}::DeJsonErr> {{
            {unit_read}
            {object_read}
        }}
    }}", name=enum_.name).parse().unwrap()
}

pub fn derive_ser_json_struct_unnamed(struct_: &Struct, crate_name: &str) -> TokenStream {
    let (bounds, args) = struct_bounds_strings(struct_, "SerJson", crate_name);
    let mut body = String::new();
    if shared::attrs_transparent(&struct_.attributes) && struct_.fields.len() == 1 {
        l!(body, "self.0.ser_json(parent)",);
    } else {
        l!(body, "let mut s = parent.new_scope()?;",);
        if struct_.fields.is_empty() {
            l!(body, "s.obj_open()?; s.obj_close()",);
        } else {
            l!(body, "s.arr_open()?;",);
            for (index, _) in struct_.fields.iter().enumerate() {
                l!(body, "s.write_element(&self.{index})?;",);
            }
            l!(body, "s.arr_close()",);
        }
    }
    let name = struct_
        .name
        .as_ref()
        .expect("Cannot implement for anonymous struct");
    format!("impl{bounds} {crate_name}::SerJson for {name}{args} {{
        fn ser_json(&self, parent: &mut {crate_name}::SerJsonScope<'_>) -> Result<(), {crate_name}::SerJsonErr> {{
            {body}
        }}
    }}").parse().unwrap()
}

pub fn derive_de_json_struct_unnamed(struct_: &Struct, crate_name: &str) -> TokenStream {
    let (bounds, args) = struct_bounds_strings(struct_, "DeJson", crate_name);
    let transparent = shared::attrs_transparent(&struct_.attributes) && struct_.fields.len() == 1;
    let mut body = String::new();
    if struct_.fields.is_empty() {
        body.push_str("s.obj_open()?; if s.object_next_entry()? { return Err(s.err_token(\"}\")); } s.obj_close()?; Ok(Self)");
    } else if transparent {
        l!(body, "Ok(Self({crate_name}::DeJson::de_json(s)?))",);
    } else {
        body.push_str("s.arr_open()?; let value = Self(");
        for _ in &struct_.fields {
            l!(body, "{{ if !s.array_next_val()? {{ return Err(s.err_token(\"array element\")); }} {crate_name}::DeJson::de_json(s)? }},",);
        }
        body.push_str("); if s.array_next_val()? { return Err(s.err_token(\"]\")); } s.arr_close()?; Ok(value)");
    }
    format!("impl{bounds} {crate_name}::DeJson for {name}{args} {{
        fn de_json_value(s: &mut {crate_name}::DeJsonScope<'_, '_>) -> ::core::result::Result<Self, {crate_name}::DeJsonErr> {{ {body} }}
    }}", name=struct_.name.as_ref().unwrap()).parse().unwrap()
}

// Save the original declaration in a small crate-local macro.
pub fn derive_json_helpers(struct_: &Struct, declaration: TokenStream) -> TokenStream {
    use proc_macro::{Delimiter, Group, TokenTree};
    let struct_name = struct_.name.as_ref().unwrap();
    let helper = format!("__nanoserde_json_metadata_{struct_name}");
    let wrapper: TokenStream = format!(
        "#[allow(unused_macros)] macro_rules! {helper} {{
            ($generator:path) => {{ $generator! {{}} }};
        }}
        #[allow(unused_imports)] pub(crate) use {helper} as {struct_name};"
    )
    .parse()
    .unwrap();
    // The only empty brace group in the wrapper is the generator's input.
    fn insert(tokens: TokenStream, declaration: &TokenStream) -> TokenStream {
        tokens
            .into_iter()
            .map(|token| match token {
                TokenTree::Group(group) => {
                    let contents =
                        if group.delimiter() == Delimiter::Brace && group.stream().is_empty() {
                            declaration.clone()
                        } else {
                            insert(group.stream(), declaration)
                        };
                    let mut replacement = Group::new(group.delimiter(), contents);
                    replacement.set_span(group.span());
                    TokenTree::Group(replacement)
                }
                other => other,
            })
            .collect()
    }
    insert(wrapper, &declaration)
}

// Local Fields/Key declarations need their own destination parameters.
fn json_helper_parameters(struct_: &Struct, crate_name: &str) -> (String, String, String, String) {
    use crate::parse::Generic;
    let mut declarations = String::new();
    let mut arguments = String::new();
    let mut parser_declarations = String::new();
    for generic in struct_.generics.iter().filter(|g| g.ident_only() != "Self") {
        l!(arguments, "{},", generic.ident_only());
        l!(declarations, "{},", generic.full_with_const(&[], true));
        let extra = if matches!(
            generic,
            Generic::Generic { .. } | Generic::WhereBounded { .. }
        ) {
            vec![format!("{crate_name}::DeJson")]
        } else {
            vec![]
        };
        l!(
            parser_declarations,
            "{},",
            generic.full_with_const(&extra.iter().map(|v| v.as_str()).collect::<Vec<_>>(), true)
        );
    }
    let destination = format!("{}<{arguments}>", struct_.name.as_ref().unwrap());
    (declarations, arguments, parser_declarations, destination)
}

pub fn generate_json_fields(struct_: &Struct, crate_name: &str) -> TokenStream {
    let (decl, args, _, destination) = json_helper_parameters(struct_, crate_name);
    let mut members = String::new();
    let mut variants = String::new();
    let mut names = String::new();
    let mut initialization = String::new();
    let mut arms = String::new();
    for field in &struct_.fields {
        let rust_field = field.field_name.as_ref().unwrap();
        let json_field = shared::attrs_rename(&field.attributes).unwrap_or(rust_field.clone());
        let ty = replace_self_type(field.ty.full().parse().unwrap(), &destination).to_string();
        l!(members, "{rust_field}: ::core::option::Option<{ty}>,",);
        l!(
            variants,
            "{rust_field}(&'__nanoserde_field mut ::core::option::Option<{ty}>),",
        );
        l!(names, "{json_field:?},",);
        l!(
            initialization,
            "{rust_field}: ::core::option::Option::None,",
        );
        let duplicate = format!("duplicate field {json_field}");
        l!(
            arms,
            "{json_field:?} => {{
            if self.{rust_field}.is_some() {{ return Err(scope.err_parse({duplicate:?})); }}
            Ok(Some(Key::{rust_field}(&mut self.{rust_field})))
        }},",
        );
    }
    format!(
        "#[allow(dead_code)]
        struct Fields<{decl}> {{ {members} }}
        #[allow(non_camel_case_types, dead_code)]
        enum Key<'__nanoserde_field, {decl}> {{
            {variants}
            __nanoserde_uninhabited(::core::convert::Infallible,
                ::core::marker::PhantomData<&'__nanoserde_field mut Fields<{args}>>),
        }}
        #[allow(dead_code)]
        impl<{decl}> Fields<{args}> {{
            const NAMES: &'static [&'static str] = &[{names}];
            fn new() -> Self {{ Self {{ {initialization} }} }}
            fn match_next_key<'__nanoserde_field>(
                &'__nanoserde_field mut self,
                scope: &mut {crate_name}::DeJsonScope<'_, '_>,
            ) -> ::core::result::Result<::core::option::Option<Key<'__nanoserde_field, {args}>>, {crate_name}::DeJsonErr> {{
                let Some(key) = scope.next_key(Self::NAMES)? else {{ return Ok(None); }};
                match key {{ {arms} _ => unreachable!(), }}
            }}
        }}"
    ).parse().unwrap()
}

pub fn generate_json_parser(struct_: &Struct, crate_name: &str) -> TokenStream {
    let (_, args, bounds, destination) = json_helper_parameters(struct_, crate_name);
    let mut arms = String::new();
    for field in &struct_.fields {
        let rust_field = field.field_name.as_ref().unwrap();
        let ty = replace_self_type(field.ty.full().parse().unwrap(), &destination).to_string();
        let body = if shared::attrs_skip(&field.attributes) {
            "let _ = destination; scope.skip_value()?;".to_string()
        } else if let Some(proxy) = shared::attrs_proxy(&field.attributes) {
            if field.ty.base() == "Option" {
                format!("let value = <::core::option::Option<{proxy}> as {crate_name}::DeJson>::de_json(scope)?;
                    *destination = Some(value.as_ref().map(::core::convert::From::from));")
            } else {
                format!(
                    "let value = <{proxy} as {crate_name}::DeJson>::de_json(scope)?;
                    *destination = Some(<{ty} as ::core::convert::From<&{proxy}>>::from(&value));"
                )
            }
        } else {
            format!("*destination = Some(<{ty} as {crate_name}::DeJson>::de_json(scope)?);")
        };
        l!(arms, "Key::{rust_field}(destination) => {{ {body} }},",);
    }
    format!(
        "struct Parser;
        impl Parser {{
            #[allow(unreachable_code)]
            fn parse_field<{bounds}>(
                key: Key<'_, {args}>,
                scope: &mut {crate_name}::DeJsonScope<'_, '_>,
            ) -> ::core::result::Result<(), {crate_name}::DeJsonErr> {{
                match key {{
                    {arms}
                    Key::__nanoserde_uninhabited(never, _) => match never {{}},
                }}
                Ok(())
            }}
        }}"
    )
    .parse()
    .unwrap()
}

// Self in a destination field means the destination, not the local Fields/Parser.
fn replace_self_type(tokens: TokenStream, destination: &str) -> TokenStream {
    use proc_macro::{Group, TokenTree};
    let mut out = TokenStream::new();
    for token in tokens {
        match token {
            TokenTree::Ident(ref ident) if ident.to_string() == "Self" => {
                let parsed = destination.parse::<TokenStream>().unwrap();
                out.extend(parsed);
            }
            TokenTree::Group(group) => {
                let mut result = Group::new(
                    group.delimiter(),
                    replace_self_type(group.stream(), destination),
                );
                result.set_span(group.span());
                out.extend(TokenStream::from(TokenTree::Group(result)));
            }
            other => out.extend(TokenStream::from(other)),
        }
    }
    out
}
