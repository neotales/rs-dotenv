# neotales-dotenv

Parse dotenv text into source-ordered nodes while preserving the original document.

By [x0craft](https://github.com/x0craft).

[![Neotales](https://github.com/neotales/brand/raw/main/assets/logo.png)](https://github.com/neotales/rs-dotenv)

<!-- badges: crates.io, docs.rs, CI. Add once the crate name and first version exist. -->

## Install 📦

```sh
cargo add neotales-dotenv
```

## What you can do 🔧

- 🧾 **Keep the source and inspect parsed assignments.** `parse` retains the original text, including line endings. `Document::assignments` yields assignments in source order.
- 📍 **Report malformed input.** `Document::first_invalid` returns the first parse message and its source span. `Document::nodes` also exposes every invalid node so a caller can report all of them.
- 🔤 **Keep value syntax visible.** Assignments retain their quote style and `export` prefix. Quoted values may span lines. `$NAME` stays literal. This parser does no I/O, interpolation, or key validation.

```rust
use neotales_dotenv::{Node, parse};

fn main() {
    let input = "# local settings\r\nexport APP_NAME='neotales'\r\nBROKEN\r\n";
    let document = parse(input);

    assert_eq!(document.source(), input);
    let assignment = document.assignments().next().unwrap();
    assert_eq!(assignment.key, "APP_NAME");
    assert_eq!(assignment.value, "neotales");
    assert!(assignment.exported);

    let invalid = document.nodes().iter().find_map(|node| match node {
        Node::Invalid { message, span, .. } => Some((message.as_str(), span.line)),
        _ => None,
    });
    assert_eq!(invalid, Some(("expected KEY=VALUE assignment in dotenv file", 3)));
}
```

## Licence 📜

[MIT](LICENSE.md)
