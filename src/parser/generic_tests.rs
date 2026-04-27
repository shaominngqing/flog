use super::*;

#[test]
fn parse_bracket_level_tag() {
    let p = GenericParser;
    let line = "I/flutter (1234): [INFO] [Network] GET /api/users";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.tag, "Network");
    assert!(entry.message.contains("GET"));
}

#[test]
fn parse_bracket_level_only() {
    let p = GenericParser;
    let line = "I/flutter (1234): [ERROR] Something went wrong";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "App");
}

#[test]
fn parse_plain_flutter() {
    let p = GenericParser;
    let line = "I/flutter (1234): Hello world";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::System);
    assert_eq!(entry.message, "Hello world");
}

#[test]
fn parse_empty_flutter_print() {
    let p = GenericParser;
    let line = "I/flutter (1234): ";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::System);
    assert_eq!(entry.tag, "flutter");
}

#[test]
fn parse_flutter_jni_warning() {
    let p = GenericParser;
    let line = "W/FlutterJNI(1234): some engine warning";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Warning);
    assert_eq!(entry.tag, "FlutterJNI");
}

#[test]
fn parse_dart_vm_error() {
    let p = GenericParser;
    let line = "E/DartVM  (1234): Unhandled exception";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "DartVM");
}

#[test]
fn parses_any_logcat_tag() {
    // GenericParser now accepts any logcat format — ADB filter handles noise
    let p = GenericParser;
    let entry = p.try_parse("I/System.out(1234): some output").unwrap();
    assert_eq!(entry.tag, "System.out");
    assert_eq!(entry.level, LogLevel::Info);
}

// VM Service stdout format tests (flutter: prefix instead of I/flutter (PID):)
#[test]
fn parse_vm_stdout_bracket_level_tag() {
    let p = GenericParser;
    let line = "flutter: [INFO][Network] GET /aura-lang-be/api/user-courses";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.tag, "Network");
    assert!(entry.message.contains("GET"));
}

#[test]
fn parse_vm_stdout_error() {
    let p = GenericParser;
    let line = "flutter: [ERROR][Network] x 404 /aura-lang-be/api/episodes/0 (521ms)";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "Network");
}

#[test]
fn parse_vm_stdout_plain() {
    let p = GenericParser;
    let line = "flutter: some plain message";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::System);
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "some plain message");
}

#[test]
fn parser_name_is_generic() {
    let p = GenericParser;
    assert_eq!(p.name(), "Generic");
}

#[test]
fn parse_exception_header() {
    let p = GenericParser;
    let line = "════════╡ EXCEPTION CAUGHT BY WIDGETS LIBRARY ╞════════════";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "Flutter");
    assert!(entry.message.contains("EXCEPTION"));
}

#[test]
fn parse_exception_decoration_line() {
    let p = GenericParser;
    let line = "═══════════════════════════════════════════════════════════";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "Flutter");
}

#[test]
fn parse_handler_line() {
    let p = GenericParser;
    let entry = p.try_parse("Handler: onTap").unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "Flutter");
}

#[test]
fn parse_recognizer_line() {
    let p = GenericParser;
    let entry = p.try_parse("Recognizer: TapGestureRecognizer").unwrap();
    assert_eq!(entry.level, LogLevel::Error);
}

#[test]
fn parse_the_following_line() {
    let p = GenericParser;
    let entry = p.try_parse("The following assertion was thrown").unwrap();
    assert_eq!(entry.level, LogLevel::Error);
}

#[test]
fn parse_when_the_exception_line() {
    let p = GenericParser;
    let entry = p
        .try_parse("When the exception was thrown, this was the stack:")
        .unwrap();
    assert_eq!(entry.level, LogLevel::Error);
}

#[test]
fn parse_failed_assertion_line() {
    let p = GenericParser;
    let entry = p
        .try_parse("Failed assertion: line 42: 'foo != null'")
        .unwrap();
    assert_eq!(entry.level, LogLevel::Error);
}

#[test]
fn parse_stacktrace_frame() {
    let p = GenericParser;
    let entry = p
        .try_parse("#0      MyWidget.build (package:my_app/widget.dart:10:5)")
        .unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "Flutter");
}

#[test]
fn parse_verbose_logcat() {
    let p = GenericParser;
    let entry = p.try_parse("V/MyTag  (1234): verbose message").unwrap();
    assert_eq!(entry.level, LogLevel::Verbose);
    assert_eq!(entry.tag, "MyTag");
}

#[test]
fn parse_debug_logcat() {
    let p = GenericParser;
    let entry = p.try_parse("D/MyTag  (1234): debug message").unwrap();
    assert_eq!(entry.level, LogLevel::Debug);
    assert_eq!(entry.tag, "MyTag");
}

#[test]
fn parse_fatal_logcat() {
    let p = GenericParser;
    let entry = p.try_parse("F/MyTag  (1234): fatal crash").unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "MyTag");
}

#[test]
fn ansi_escape_is_stripped_from_exception() {
    // ANSI color codes should be stripped before EXCEPTION detection
    let p = GenericParser;
    let line = "\x1b[31m════════╡ EXCEPTION CAUGHT ╞════════\x1b[0m";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    // The stripped message should not contain raw ANSI codes
    assert!(!entry.message.contains("\x1b["));
}

#[test]
fn ansi_escape_stripped_in_flutter_content() {
    let p = GenericParser;
    let line = "I/flutter (1234): \x1b[32mhello\x1b[0m";
    let entry = p.try_parse(line).unwrap();
    assert!(!entry.message.contains("\x1b["));
    assert!(entry.message.contains("hello"));
}

#[test]
fn unmatched_line_returns_none() {
    let p = GenericParser;
    assert!(p.try_parse("totally random text with no pattern").is_none());
    assert!(p.try_parse("").is_none());
}

#[test]
fn bracket_level_unknown_falls_back_to_plain_flutter() {
    // `[NOTALEVEL] ...` — bracket regex matches but LogLevel::from_str returns None;
    // falls through to plain flutter content path.
    let p = GenericParser;
    let line = "I/flutter (1234): [NOTALEVEL] hi";
    let entry = p.try_parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::System);
    assert_eq!(entry.tag, "flutter");
}

// ---- Phase 3 Step 3.1 DOM-016: flutter helper extractions ----

#[test]
fn try_parse_flutter_prefixed_accepts_i_flutter_line() {
    let entry =
        GenericParser::try_parse_flutter_prefixed("I/flutter ( 1234): hello world").unwrap();
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "hello world");
    assert_eq!(entry.level, LogLevel::System);
}

#[test]
fn try_parse_flutter_prefixed_accepts_flutter_colon_line() {
    let entry = GenericParser::try_parse_flutter_prefixed("flutter: simple message").unwrap();
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "simple message");
    assert_eq!(entry.level, LogLevel::System);
}

#[test]
fn try_parse_flutter_prefixed_lifts_bracketed_level_and_tag() {
    let entry =
        GenericParser::try_parse_flutter_prefixed("flutter: [ERROR][Net] connection failed")
            .unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.tag, "Net");
    assert_eq!(entry.message, "connection failed");
}

#[test]
fn try_parse_flutter_prefixed_empty_content_yields_system() {
    let entry = GenericParser::try_parse_flutter_prefixed("flutter: ").unwrap();
    assert_eq!(entry.level, LogLevel::System);
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "");
}

#[test]
fn try_parse_flutter_prefixed_non_flutter_line_returns_none() {
    assert!(GenericParser::try_parse_flutter_prefixed("[INFO][Tag] not flutter").is_none());
    assert!(GenericParser::try_parse_flutter_prefixed("plain message").is_none());
}

#[test]
fn try_parse_flutter_structured_requires_bracket_shape_and_known_level() {
    // No bracket → None
    assert!(GenericParser::try_parse_flutter_structured("no brackets here").is_none());
    // Bracket with unknown level → None (caller must fall back to plain)
    assert!(GenericParser::try_parse_flutter_structured("[NOTALEVEL][Tag] msg").is_none());
    // Valid bracket + known level → Some
    let e = GenericParser::try_parse_flutter_structured("[INFO][Tag] msg").unwrap();
    assert_eq!(e.level, LogLevel::Info);
    assert_eq!(e.tag, "Tag");
    assert_eq!(e.message, "msg");
}

#[test]
fn build_flutter_plain_always_system_level_tagged_flutter() {
    let e = GenericParser::build_flutter_plain("anything".to_string());
    assert_eq!(e.level, LogLevel::System);
    assert_eq!(e.tag, "flutter");
    assert_eq!(e.message, "anything");
    // Empty content is also valid (print('') case)
    let e2 = GenericParser::build_flutter_plain(String::new());
    assert_eq!(e2.message, "");
    assert_eq!(e2.level, LogLevel::System);
}
