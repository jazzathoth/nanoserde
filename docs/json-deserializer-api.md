# JSON deserialization API

JSON readers now use `DeJsonScope`. `DeJsonState` owns the input's `Chars` cursor, token, scratch buffers, and position. A scope borrows that state for one object or array. No second input argument is passed to readers.

## Reading a value

`T::deserialize_json(input)` reads one complete document, including root scalars. It rejects empty input and trailing values or punctuation. Existing whitespace skipping and comment handling remain.

`T::de_json(scope)` is the entry point for another value. Its default implementation creates a child scope when the current token is `{` or `[`, calls the implementation hook, and checks completion before dropping the child. Scalar values use the enclosing scope.

Implement `de_json_value` for normal custom readers. `Option<T>` handles null and otherwise delegates to `T::de_json`; Box also delegates without creating an extra container scope. Generated transparent/proxy wrappers inherit the default `de_json` and delegate through the public `de_json` entry point. Their unused outer scope can finish after the delegated reader completes the same value.

```rust
use nanoserde::{DeJson, DeJsonErr, DeJsonScope};

struct Positive(u8);
impl DeJson for Positive {
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        let value = scope.parse_u8()?;
        if value == 0 {
            return Err(scope.err_parse("expected a positive number"));
        }
        Ok(Self(value))
    }
}
```

## Token movement

- `token()`, `is_string()`, `is_null()`, `check_token(...)`, and opener checks inspect the current token without advancing.
- `next_tok()` advances once and enforces the active scope's boundary. `next_token_is(...)` advances then checks.
- `parse_string`, `parse_bool`, `parse_null`, and the numeric `parse_*` methods read the current scalar and advance to the following token.
- `obj_open()` and `arr_open()` require their corresponding current opener without advancing. On success they set `can_finish` to false; a mismatch errors before changing scope state.
- `obj_close()` and `arr_close()` validate the current scope's completion without advancing.
- A completed child leaves its closing token current. The parent's next navigation call advances past that child's closer using the parent's own boundary checks.
- `done` prevents advancing beyond the current scope's end. A comma does not finish a container scope.
- `can_finish` starts true, allowing an unused outer scope to finish after delegation. Opening or advancing through that scope sets it false. Reaching its matching closer sets both `done` and `can_finish` true. Finish always requires the matching current closer as well, so returning without reading anything still fails.


Malformed tokens are tokenizer errors; missing/repeated/trailing commas are navigation errors. The scanner retains existing lexical extensions; strict separators do not mean every old tokenizer extension has been removed.

The root scope starts on `Bof` and ends on `Eof`. The convenience method primes the first token, reads one value, advances past a completed root container if needed, then requires EOF. As in the original tokenizer, NUL terminates input and produces Eof. An unfinished container still errors because it requires its matching closer.

## Object keys and arrays

`scope.next_key(&["name", "age"])` handles object entry, separators, string-key recognition, and the colon. It returns a canonical string reference from that supplied list, leaving the value current. It returns None at the object's end. A returned canonical key does not borrow scope's string buffer.

`scope.next_key_string()` performs the same navigation but returns an owned unrestricted string key, for manual map-like reading. Existing map implementations retain their generic key readers and prior supported key representations, while sharing the same separator navigation.

`scope.array_next_val()` returns true with an element current, or false at the enclosing array's end:

```rust
scope.arr_open()?;
let mut values = Vec::new();
while scope.array_next_val()? {
    values.push(Item::de_json(scope)?);
}
scope.arr_close()?;
```

`skip_value()` consumes a complete scalar or nested value under these same movement rules. Custom handlers must read or skip the selected value before continuing their loop.

## Generated helpers for a manual implementation

Annotate the original named struct with `DeJsonHelpers`. This generates only crate-local metadata, exposed as a macro with the same name as the struct. It does not implement DeJson, change the struct, or declare module-level Fields/Key/Parser types.

Inside the implementation, `fields_for!(Person)` declares local Fields and Key. `parser_for!(Person)` declares local Parser from the same metadata. Neither invocation repeats member names or types.

Each entry macro invokes the destination macro with a separate final procedural generator: `generate_json_fields_impl` or `generate_json_parser_impl`. That generator parses the saved original declaration and directly produces the local definitions. See [the expansion and source map](json-helper-generation-plan.md) and [a compiled expanded example](../tests/json_helpers_expanded.rs).

```rust
use nanoserde::{fields_for, parser_for, DeJson, DeJsonErr, DeJsonHelpers, DeJsonScope};

#[derive(DeJsonHelpers)]
struct Person {
    name: String,
    age: u8,
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
        // Additional validation or a constructor call can happen here.
        Ok(Self { name, age })
    }
}
```

Each Fields member is `Option<T>`. Key variants carry `&mut Option<T>` for the corresponding member. An optional destination therefore has `Option<Option<T>>` storage, distinguishing absent input from explicit null.

Fields rejects unknown names and rejects duplicates when storage is already Some. Duplicate errors currently use CannotParse. There is no independent seen set: skipping a key or taking its stored value can leave it eligible to appear again. A custom handler controls that behavior. Explicit `#[nserde(skip)]` fields are known, discarded by the generated Parser, and not populated.

`#[nserde(rename = "...")]` controls wire names; Key variants and Fields members keep the Rust field identifiers. Field proxies are applied by Parser. Defaults and final construction remain the handwritten implementation's responsibility; the helpers do not fill defaults automatically.

For a generic declaration such as `Envelope<T>`, the macros declare their own generic Fields/Key types. Use `Fields::<T>::new()` if inference cannot select the desired type. Lifetime and const parameters work the same way. Self in a field type refers to the destination, not the generated helper.

For multiple destination types in one module, each implementation gets its own local Fields, Key, and Parser. An import of the type can also import its crate-local metadata macro. When expanding helpers in a different module, field type names and aliases used by the declaration must be visible there too, or be fully qualified in the declaration. These macro expansions follow ordinary Rust macro name resolution; they do not perform reflection or resolve aliases themselves.

DeJsonHelpers currently accepts named structs. Ordinary DeJson derives continue to handle their existing named/tuple structs and enums. Binary, RON, TOML, and JSON serialization APIs are unchanged. JSON unknown fields and duplicate populated fields now error; trailing commas no longer pass. Existing explicitly skipped fields remain skipped.
