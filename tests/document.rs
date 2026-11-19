//! Behaviour tests for the public document API.

use neotales_dotenv::{Document, Node, QuoteStyle, parse};

const GO_README_SAMPLE: &str = r#"KEY1="value1"

KEY2='value2'
Key3=value3
Key4=a value with spaces
# This is a comment
Key5="a value with \"escaped quotes\""
Key6='a value with \'single quotes\''
Key7="line1
line2
line3
"
Key8="value with \nnewlines"
Key9="value with \t tabs"
Key11="😈"
"#;

fn quote_of(document: &Document, key: &str) -> Option<QuoteStyle> {
    document
        .assignments()
        .find(|assignment| assignment.key == key)
        .map(|assignment| assignment.quote)
}

#[test]
fn build_adds_variables_comments_and_newlines() {
    let mut document = Document::new();
    document.add_variable("KEY1", "value1");
    document.add_comment("This is a comment");
    document.add_newline();
    document.add_variable("KEY2", "value2");

    assert_eq!(document.len(), 4);
    assert!(!document.is_empty());
    assert_eq!(document.get("KEY1"), Some("value1"));
    assert_eq!(document.get("KEY2"), Some("value2"));
    assert_eq!(
        document.comments().collect::<Vec<_>>(),
        ["This is a comment"]
    );
    assert_eq!(document.keys().collect::<Vec<_>>(), ["KEY1", "KEY2"]);

    let map = document.to_map();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("KEY1").map(String::as_str), Some("value1"));
    assert_eq!(map.get("KEY2").map(String::as_str), Some("value2"));
    assert!(!map.contains_key("This is a comment"));

    assert!(
        matches!(document.at(0), Some(Node::Assignment(assignment)) if assignment.key == "KEY1")
    );
    assert!(matches!(document.at(1), Some(Node::Comment { .. })));
    assert!(matches!(document.at(2), Some(Node::Blank { .. })));
    assert!(matches!(document.at(3), Some(Node::Assignment(_))));
    assert!(document.at(4).is_none());
    assert!(document.at(usize::MAX).is_none());
}

#[test]
fn keys_follow_the_order_variables_were_added() {
    let mut document = Document::new();
    document.add_variable("ZED", "1");
    document.add_comment("a comment");
    document.add_variable("ALPHA", "2");
    document.add_variable("MIDDLE", "3");

    assert_eq!(
        document.keys().collect::<Vec<_>>(),
        ["ZED", "ALPHA", "MIDDLE"]
    );
    assert_eq!(
        document.to_map().keys().collect::<Vec<_>>(),
        ["ZED", "ALPHA", "MIDDLE"]
    );
}

#[test]
fn get_of_a_missing_key_is_none() {
    let document = parse("ONE=1\n");
    assert_eq!(document.get("TWO"), None);
    assert_eq!(Document::new().get("ONE"), None);
}

#[test]
fn set_inserts_and_updates_without_moving_a_key() {
    let mut document = parse("A=1\nB=2\n");

    document.set("B", "9");
    document.set("C", "3");

    assert_eq!(document.get("A"), Some("1"));
    assert_eq!(document.get("B"), Some("9"));
    assert_eq!(document.get("C"), Some("3"));
    assert_eq!(document.keys().collect::<Vec<_>>(), ["A", "B", "C"]);
    assert_eq!(document.len(), 3);
    assert_eq!(document.to_string(), "A=1\nB=9\nC=3\n");
}

#[test]
fn keys_keep_order_after_set_calls() {
    let mut document = Document::new();
    document.set("B", "1");
    document.set("A", "2");
    document.set("B", "3");
    document.set("C", "4");

    assert_eq!(document.keys().collect::<Vec<_>>(), ["B", "A", "C"]);
    assert_eq!(document.get("B"), Some("3"));
    assert_eq!(document.get("A"), Some("2"));
    assert_eq!(document.get("C"), Some("4"));
}

#[test]
fn set_quotes_a_value_that_unquoted_syntax_cannot_carry() {
    let mut document = parse("A=1\n");

    document.set("A", "has # hash");

    assert_eq!(document.get("A"), Some("has # hash"));
    assert_eq!(document.to_string(), "A=\"has # hash\"\n");
    assert_eq!(parse(&document.to_string()).get("A"), Some("has # hash"));
}

#[test]
fn merge_overwrites_adds_and_does_not_copy_comments() {
    let mut left = parse("# left\nA=1\nB=2\n");
    let right = parse("# right comment\nD=4\nB=20\nC=3\n");

    left.merge(&right);

    assert_eq!(left.get("A"), Some("1"));
    assert_eq!(left.get("B"), Some("20"));
    assert_eq!(left.get("C"), Some("3"));
    assert_eq!(left.get("D"), Some("4"));
    assert_eq!(left.keys().collect::<Vec<_>>(), ["A", "B", "D", "C"]);
    assert_eq!(left.comments().collect::<Vec<_>>(), ["left"]);
    assert_eq!(left.to_string(), "# left\nA=1\nB=20\nD=4\nC=3\n");
}

#[test]
fn to_map_keeps_the_last_value_for_a_repeated_key() {
    let document = parse("A=1\nA=2\nB=3\n");

    assert_eq!(document.keys().collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(document.get("A"), Some("2"));

    let map = document.to_map();
    assert_eq!(map.get("A").map(String::as_str), Some("2"));
    assert_eq!(map.get("B").map(String::as_str), Some("3"));
    assert_eq!(map.keys().collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(map.len(), 2);
    assert_eq!(document.to_string(), "A=1\nA=2\nB=3\n");
}

#[test]
fn parses_double_and_single_quoted_values() {
    let document = parse("DOUBLE=\"double value\"\nSINGLE='single value'\n");

    assert_eq!(document.get("DOUBLE"), Some("double value"));
    assert_eq!(document.get("SINGLE"), Some("single value"));
    assert_eq!(quote_of(&document, "DOUBLE"), Some(QuoteStyle::Double));
    assert_eq!(quote_of(&document, "SINGLE"), Some(QuoteStyle::Single));
}

#[test]
fn empty_value_is_kept_as_an_empty_string() {
    let document = parse("EMPTY=\nNEXT=1\n");

    assert_eq!(document.get("EMPTY"), Some(""));
    assert_eq!(document.keys().collect::<Vec<_>>(), ["EMPTY", "NEXT"]);

    let last = parse("LAST=");
    assert_eq!(last.get("LAST"), Some(""));
}

#[test]
fn parses_underscore_keys_spaces_and_special_characters() {
    let document = parse(
        "KEY_WITH_UNDERSCORE=value\nKEY_WITH_SPACES=value with spaces\nKEY_WITH_SPECIAL_CHARS=value@!%\n",
    );

    assert_eq!(document.get("KEY_WITH_UNDERSCORE"), Some("value"));
    assert_eq!(document.get("KEY_WITH_SPACES"), Some("value with spaces"));
    assert_eq!(document.get("KEY_WITH_SPECIAL_CHARS"), Some("value@!%"));
}

#[test]
fn double_quoted_escapes_decode() {
    let document = parse(
        "NEWLINE=\"value with \\nnewlines\"\nTABS=\"value with \\ttabs\"\nQUOTES=\"value with \\\"escaped quotes\\\"\"\nUNICODE=\"value with \\U0001F920\"\n",
    );

    assert_eq!(document.get("NEWLINE"), Some("value with \nnewlines"));
    assert_eq!(document.get("TABS"), Some("value with \ttabs"));
    assert_eq!(
        document.get("QUOTES"),
        Some("value with \"escaped quotes\"")
    );
    assert_eq!(document.get("UNICODE"), Some("value with 🤠"));
}

#[test]
fn multiline_quoted_value_keeps_its_newlines() {
    let document = parse("MULTI=\"line1\nline2\nline3\n\"\n");

    assert_eq!(document.get("MULTI"), Some("line1\nline2\nline3\n"));
    let assignment = document.assignments().next().unwrap();
    assert_eq!(assignment.span.end_line, 4);
}

#[test]
fn mixed_document_keeps_comment_blank_line_and_values() {
    let document = parse("DOUBLE=\"one\"\nSINGLE='two'\n# comment\n\nPLAIN=three\n");

    assert_eq!(document.get("DOUBLE"), Some("one"));
    assert_eq!(document.get("SINGLE"), Some("two"));
    assert_eq!(document.get("PLAIN"), Some("three"));
    assert_eq!(document.comments().collect::<Vec<_>>(), ["comment"]);
    assert!(matches!(document.at(3), Some(Node::Blank { .. })));
    assert_eq!(
        document.keys().collect::<Vec<_>>(),
        ["DOUBLE", "SINGLE", "PLAIN"]
    );
}

#[test]
fn unquoted_unicode_and_emoji_are_kept() {
    let document = parse("KEY_WITH_UNICODE=こんにちは\nKEY_WITH_EMOJI=😊\nKEY_WITH_DEVIL=😈\n");

    assert_eq!(document.get("KEY_WITH_UNICODE"), Some("こんにちは"));
    assert_eq!(document.get("KEY_WITH_EMOJI"), Some("😊"));
    assert_eq!(document.get("KEY_WITH_DEVIL"), Some("😈"));
}

#[test]
fn single_quoted_backslash_n_stays_two_characters() {
    let document = parse("LITERAL='value with single quote \\n'\n");
    let value = document.get("LITERAL").unwrap();

    assert_eq!(value, "value with single quote \\n");
    assert!(!value.contains('\n'));
    assert!(value.ends_with('n'));
}

#[test]
fn export_prefix_is_not_part_of_the_key() {
    let input = "KEY=value\nexport EXPORTED=other\nexport=value\n";
    let document = parse(input);

    assert_eq!(document.get("KEY"), Some("value"));
    assert_eq!(document.get("EXPORTED"), Some("other"));
    assert_eq!(document.get("export"), Some("value"));
    assert_eq!(document.get("KEY "), None);
    assert_eq!(
        document.keys().collect::<Vec<_>>(),
        ["KEY", "EXPORTED", "export"]
    );
    assert_eq!(document.to_string(), input);
}

#[test]
fn export_accepts_tabs_before_the_key() {
    let document = parse("export\tTABBED=tabbed\n");

    assert_eq!(document.get("TABBED"), Some("tabbed"));
    assert_eq!(document.to_string(), "export TABBED=tabbed\n");
}

#[test]
fn exported_quoted_value_keeps_prefix_and_inline_comment() {
    let document = parse("export KEY=\"quoted\" # note\n");

    assert_eq!(document.get("KEY"), Some("quoted"));
    assert_eq!(document.comments().collect::<Vec<_>>(), ["note"]);
    assert_eq!(document.to_string(), "export KEY=\"quoted\" # note\n");

    let assignment = document.assignments().next().unwrap();
    assert!(assignment.exported);
    assert_eq!(assignment.quote, QuoteStyle::Double);
}

#[test]
fn export_without_a_key_is_not_a_variable() {
    let document = parse("export\nKEY=value\n");

    assert_eq!(document.keys().collect::<Vec<_>>(), ["KEY"]);
    assert_eq!(document.get("export"), None);
    assert_eq!(
        document.first_invalid().map(|(message, _)| message),
        Some("expected KEY=VALUE assignment in dotenv file")
    );
}

#[test]
fn add_exported_variable_writes_the_prefix() {
    let mut document = Document::new();
    document.add_exported_variable("KEY", "value");

    assert_eq!(document.get("KEY"), Some("value"));
    assert_eq!(document.to_string(), "export KEY=value");

    let mut parsed = parse("export KEY=old\n");
    parsed.add_exported_variable("KEY", "new");
    assert_eq!(parsed.to_string(), "export KEY=new\n");
}

#[test]
fn go_readme_sample_keys_and_values() {
    let document = parse(GO_README_SAMPLE);

    assert_eq!(
        document.keys().collect::<Vec<_>>(),
        [
            "KEY1", "KEY2", "Key3", "Key4", "Key5", "Key6", "Key7", "Key8", "Key9", "Key11",
        ]
    );

    assert_eq!(document.get("KEY1"), Some("value1"));
    assert_eq!(document.get("KEY2"), Some("value2"));
    assert_eq!(document.get("Key3"), Some("value3"));
    assert_eq!(document.get("Key4"), Some("a value with spaces"));
    assert_eq!(
        document.get("Key5"),
        Some("a value with \"escaped quotes\"")
    );
    assert_eq!(document.get("Key6"), Some("a value with 'single quotes'"));
    assert_eq!(document.get("Key7"), Some("line1\nline2\nline3\n"));
    assert_eq!(document.get("Key8"), Some("value with \nnewlines"));
    assert_eq!(document.get("Key9"), Some("value with \t tabs"));
    assert_eq!(document.get("Key11"), Some("😈"));
    assert_eq!(document.get("MISSING"), None);

    assert_eq!(
        document.comments().collect::<Vec<_>>(),
        ["This is a comment"]
    );

    assert_eq!(quote_of(&document, "KEY1"), Some(QuoteStyle::Double));
    assert_eq!(quote_of(&document, "KEY2"), Some(QuoteStyle::Single));
    assert_eq!(quote_of(&document, "Key3"), Some(QuoteStyle::Unquoted));
    assert_eq!(quote_of(&document, "Key4"), Some(QuoteStyle::Unquoted));
    assert_eq!(quote_of(&document, "Key5"), Some(QuoteStyle::Double));
    assert_eq!(quote_of(&document, "Key6"), Some(QuoteStyle::Single));
    assert_eq!(quote_of(&document, "Key7"), Some(QuoteStyle::Double));
    assert_eq!(quote_of(&document, "Key8"), Some(QuoteStyle::Double));
    assert_eq!(quote_of(&document, "Key9"), Some(QuoteStyle::Double));
    assert_eq!(quote_of(&document, "Key11"), Some(QuoteStyle::Double));
}

#[test]
fn inline_comment_is_not_part_of_the_value_and_is_written_back_on_its_line() {
    let input = "KEY=value # end comment\nKEY2=\"quoted\" # end comment\nKEY3='single' # end comment\nKEY4=\"a # b\"\n# full line comment\n";
    let document = parse(input);

    assert_eq!(document.get("KEY"), Some("value"));
    assert_eq!(document.get("KEY2"), Some("quoted"));
    assert_eq!(document.get("KEY3"), Some("single"));
    assert_eq!(document.get("KEY4"), Some("a # b"));
    assert_eq!(
        document.comments().collect::<Vec<_>>(),
        [
            "end comment",
            "end comment",
            "end comment",
            "full line comment"
        ]
    );
    assert_eq!(document.to_string(), input);
}

#[test]
fn unquoted_hash_ends_the_value_but_not_the_comment() {
    let document = parse("PLAIN=a#b # aside\nDOUBLE=\"x#y\"\nSINGLE='x#y'\n");

    assert_eq!(document.get("PLAIN"), Some("a"));
    assert_eq!(document.comments().collect::<Vec<_>>(), ["b # aside"]);
    assert_eq!(document.get("DOUBLE"), Some("x#y"));
    assert_eq!(document.get("SINGLE"), Some("x#y"));
}

#[test]
fn display_keeps_the_quote_and_the_inline_comment() {
    let document = parse("KEY=\"a \\\"quoted\\\" value\" # aside\n");

    assert_eq!(document.get("KEY"), Some("a \"quoted\" value"));
    assert_eq!(
        document.to_string(),
        "KEY=\"a \\\"quoted\\\" value\" # aside\n"
    );
}

#[test]
fn display_of_the_sample_reparses_to_the_same_values() {
    let document = parse(GO_README_SAMPLE);
    let written = document.to_string();
    let reparsed = parse(&written);

    assert_eq!(reparsed.to_map(), document.to_map());
    assert_eq!(
        reparsed.keys().collect::<Vec<_>>(),
        document.keys().collect::<Vec<_>>()
    );
}

#[test]
fn built_document_round_trips_through_display() {
    let mut document = Document::new();
    document.add_newline();
    document.add_comment("note");
    document.add_variable("KEY", "value");
    document.add_quoted_variable("QUOTED", "q", QuoteStyle::Double);

    assert_eq!(document.to_string(), "\n# note\nKEY=value\nQUOTED=\"q\"");

    let reparsed = parse(&document.to_string());
    assert_eq!(reparsed.get("KEY"), Some("value"));
    assert_eq!(reparsed.get("QUOTED"), Some("q"));
    assert_eq!(reparsed.comments().collect::<Vec<_>>(), ["note"]);
}

#[test]
fn add_inline_comment_attaches_to_the_last_variable() {
    let mut document = Document::new();
    document.add_variable("KEY", "value");
    document.add_inline_comment("note");

    assert_eq!(document.to_string(), "KEY=value # note");
    assert_eq!(document.get("KEY"), Some("value"));
    assert_eq!(document.comments().collect::<Vec<_>>(), ["note"]);
}

#[test]
fn add_variable_quotes_values_that_need_it() {
    let cases = [
        ("QUOTE", "a\"b"),
        ("APOSTROPHE", "a'b"),
        ("BACKSLASH", "a\\b"),
        ("NEWLINE", "a\nb"),
        ("TAB", "a\tb"),
        ("VERTICAL_TAB", "a\u{b}b"),
        ("FORM_FEED", "a\u{c}b"),
        ("CARRIAGE_RETURN", "a\rb"),
        ("BACKSPACE", "a\u{8}b"),
    ];

    let mut document = Document::new();
    for (key, value) in cases {
        document.add_variable(key, value);
    }
    let text = document.to_string();

    for (key, value) in cases {
        let line = text
            .lines()
            .find(|line| line.starts_with(&format!("{key}=")))
            .unwrap();
        assert!(line.contains('"'), "{key} should be quoted in {line:?}");
        assert_eq!(parse(&text).get(key), Some(value), "{key} must round-trip");
    }

    let mut plain = Document::new();
    plain.add_variable("SPACED", "a value with spaces");
    plain.add_variable("WRAPPED", "\"already\"");
    assert_eq!(
        plain.to_string(),
        "SPACED=a value with spaces\nWRAPPED=\"already\""
    );
}

#[test]
fn add_variable_keeps_a_literal_unicode_escape_as_text() {
    let mut document = Document::new();
    document.add_variable("SNOWMAN", "\\u2603");

    assert_eq!(document.get("SNOWMAN"), Some("\\u2603"));

    let text = document.to_string();
    assert_eq!(text, "SNOWMAN=\"\\\\u2603\"");
    assert_eq!(parse(&text).get("SNOWMAN"), Some("\\u2603"));
}

#[test]
fn to_exports_prefixes_every_variable_and_leaves_to_string_alone() {
    let mut document = Document::new();
    document.add_variable("APP_NAME", "neotales");
    document.add_exported_variable("DEBUG", "true");

    assert_eq!(
        document.to_exports(),
        "export APP_NAME=neotales\nexport DEBUG=true"
    );
    assert_eq!(document.to_string(), "APP_NAME=neotales\nexport DEBUG=true");
}

#[test]
fn to_exports_keeps_comments_quotes_and_blank_lines() {
    let document = parse("# full\nexport KEY=\"value\" # note\n\nPLAIN=one\n");

    assert_eq!(
        document.to_exports(),
        "# full\nexport KEY=\"value\" # note\n\nexport PLAIN=one\n"
    );
    assert_eq!(
        document.to_string(),
        "# full\nexport KEY=\"value\" # note\n\nPLAIN=one\n"
    );
}

#[test]
fn to_exports_keeps_an_inline_comment_on_the_exported_line() {
    let mut document = Document::new();
    document.add_variable("KEY", "value");
    document.add_inline_comment("note");

    assert_eq!(document.to_exports(), "export KEY=value # note");
    assert_eq!(document.to_string(), "KEY=value # note");
}

#[test]
fn parsed_document_writes_the_same_inline_comment_after_an_edit() {
    let mut document = parse("A=1 # keep me\n# full\nB=2\n");
    document.set("A", "10");

    assert_eq!(document.to_string(), "A=10 # keep me\n# full\nB=2\n");
    assert_eq!(document.comments().collect::<Vec<_>>(), ["keep me", "full"]);
}
