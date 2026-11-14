//! Additional tests for host integration, attributes, locking and error paths.

use crate::*;

struct EmptyHost;

impl VimHost for EmptyHost {}

fn run_with_host(source: &str, host: &mut dyn VimHost) -> Result<()> {
    let mut runtime = VimRuntime::new();
    execute_script(source.as_bytes(), "test.vim", &mut runtime, host)
}

fn run(source: &str) -> VimRuntime {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    execute_script(source.as_bytes(), "test.vim", &mut runtime, &mut host).unwrap();
    runtime
}

fn fail(source: &str) -> VimError {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    execute_script(source.as_bytes(), "test.vim", &mut runtime, &mut host).unwrap_err()
}

fn eval(runtime: &mut VimRuntime, input: &str) -> Value {
    let mut host = BasicHost::new();
    eval_expression(input.as_bytes(), runtime, &mut host).unwrap()
}

fn items(value: &Value) -> Vec<String> {
    value
        .as_list()
        .unwrap()
        .borrow()
        .items
        .iter()
        .map(Value::echo_string)
        .collect()
}

#[test]
fn interpolated_strings_and_dynamic_let_names() {
    let mut runtime = run("let x = 5
let name = 'dyn'
let g:{name}_suffix = 42");
    assert_eq!(eval(&mut runtime, "$'lit {x}'").echo_string(), "lit 5");
    assert_eq!(
        eval(&mut runtime, "$\"double {x + 1}\"").echo_string(),
        "double 6"
    );
    assert_eq!(
        eval(&mut runtime, "$'{{braces}}'").echo_string(),
        "{braces}"
    );
    assert_eq!(eval(&mut runtime, "g:dyn_suffix").echo_string(), "42");
    assert_eq!(
        eval(&mut runtime, "$'the square root of {{9}} is {sqrt(9)}'").echo_string(),
        "the square root of {9} is 3.0"
    );
}

#[test]
fn host_defaults_are_object_safe_and_report_unsupported() {
    let mut host = EmptyHost;
    let error = host.get_line(1).unwrap_err();
    assert!(error.to_string().contains("does not support"));
    let error = host
        .execute_ex(
            "set number",
            &ExContext::new(rvim_types::ScriptId::from_raw(1), "x", 1, None),
        )
        .unwrap_err();
    assert_eq!(error.code, Some(492));
    assert!(host.get_register('a').is_err());
    assert!(!host.function_exists("whatever"));
}

#[test]
fn host_ex_command_errors_are_catchable() {
    let mut host = EmptyHost;
    run_with_host(
        "let g:caught = 0\ntry\n  set number\ncatch /E492/\n  let g:caught = 1\nendtry\n",
        &mut host,
    )
    .unwrap();
    // the host cannot store variables, so use a host that can for inspectable
    // state; this test only proves the command ran without an uncaught error.
}

#[test]
fn host_ex_errors_are_catchable_with_basic_host_override() {
    struct FailingHost {
        inner: BasicHost,
    }
    impl VimHost for FailingHost {
        fn set_var(&mut self, scope: VarScope, name: &str, value: Value) -> Result<()> {
            self.inner.set_var(scope, name, value)
        }
        fn get_var(&mut self, scope: VarScope, name: &str) -> Result<Value> {
            self.inner.get_var(scope, name)
        }
        fn execute_ex(&mut self, command: &str, _context: &ExContext) -> Result<Value> {
            Err(VimError::new(
                492,
                format!("Not an editor command: {command}"),
            ))
        }
    }
    let mut host = FailingHost {
        inner: BasicHost::new(),
    };
    let mut runtime = VimRuntime::new();
    execute_script(
        b"let g:caught = 0\ntry\n  set number\ncatch /E492/\n  let g:caught = 1\nendtry\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(runtime.get_global("caught"), Some(Value::Number(1)));
}

#[test]
fn host_ex_commands_receive_context() {
    struct RecordingHost {
        commands: Vec<String>,
        context: Option<ExContext>,
    }
    impl VimHost for RecordingHost {
        fn execute_ex(&mut self, command: &str, context: &ExContext) -> Result<Value> {
            self.commands.push(command.to_string());
            self.context = Some(context.clone());
            Ok(Value::Number(0))
        }
    }
    let mut host = RecordingHost {
        commands: Vec::new(),
        context: None,
    };
    run_with_host(
        "let g:x = 1\nset number\nfunction! F()\n  setlocal wrap\nendfunction\ncall F()\n",
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.commands,
        vec!["set number".to_string(), "setlocal wrap".to_string()]
    );
    let context = host.context.unwrap();
    assert_eq!(context.line, 4);
    assert_eq!(context.function.as_deref(), Some("F"));
}

#[test]
fn silent_modifiers_suppress_messages() {
    let (mut runtime, host) = {
        let mut runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        execute_script(
            b"silent echo 'quiet'\nsilent! echo 'quiet2'\nverbose echo 'loud'\n",
            "test.vim",
            &mut runtime,
            &mut host,
        )
        .unwrap();
        (runtime, host)
    };
    assert_eq!(host.messages, vec!["loud".to_string()]);
    let _ = &mut runtime;
}

#[test]
fn function_redefinition_and_name_rules() {
    assert_eq!(
        fail("function! F()\nendfunction\nfunction F()\nendfunction\n").code,
        Some(122)
    );
    assert_eq!(fail("function! lowercase()\nendfunction\n").code, Some(128));
    assert_eq!(fail("function! len()\nendfunction\n").code, Some(705));
    assert_eq!(fail("delfunction Missing\n").code, Some(130));
    let runtime = run(
        "function! foo#bar()\n  return 7\nendfunction\nlet g:x = foo#bar()\nlet g:fp = function('foo#bar')\nlet g:y = g:fp()\n",
    );
    assert_eq!(runtime.get_global("x"), Some(Value::Number(7)));
    assert_eq!(runtime.get_global("y"), Some(Value::Number(7)));
}

#[test]
fn function_body_errors_are_reported_at_call_time() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    execute_script(
        b"function! Bad()\n  if 1\nendfunction\nlet g:defined = 1\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(runtime.get_global("defined"), Some(Value::Number(1)));
    let error = eval_expression(b"Bad()", &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(171));
}

#[test]
fn range_function_attribute_is_explicitly_unsupported() {
    let error = fail("function! F() range\nendfunction\n");
    assert!(error.to_string().contains("range"));
}

#[test]
fn break_continue_return_outside_blocks_error() {
    assert_eq!(fail("break\n").code, Some(587));
    assert_eq!(fail("continue\n").code, Some(588));
    assert_eq!(fail("return 1\n").code, Some(133));
    assert_eq!(
        fail("function! F()\n  break\nendfunction\ncall F()\n").code,
        Some(587)
    );
}

#[test]
fn maxfuncdepth_is_enforced() {
    let mut runtime = VimRuntime::new();
    runtime.set_maxfuncdepth(2);
    let mut host = BasicHost::new();
    let error = execute_script(
        b"function! Recurse()\n  return Recurse()\nendfunction\ncall Recurse()\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap_err();
    assert_eq!(error.code, Some(132));
}

#[test]
fn unlet_dictionary_entries_and_bad_targets() {
    let runtime = run(
        "let g:d = {'a': 1, 'b': 2}\nunlet g:d.a\nunlet g:d['b']\nlet g:len = len(g:d)\nunlet! g:d.missing\nlet g:l = [1]\n",
    );
    assert_eq!(runtime.get_global("len"), Some(Value::Number(0)));
    let error = fail("let g:l = [1]\nunlet g:l[0]\n");
    assert_eq!(error.code, Some(15));
    let error = fail("let g:d = {}\nunlet g:d.missing\n");
    assert_eq!(error.code, Some(716));
}

#[test]
fn extend_remove_blob_and_get_defaults() {
    let runtime = run(
        "let g:d = {'a': 1}\ncall extend(g:d, {'b': 2})\nlet g:removed = remove(g:d, 'a')\nlet g:getb = get(0z0102, 5, 9)\nlet g:b = 0z0102\nlet g:rb = reverse(g:b)\nlet g:b0 = g:b[0]\n",
    );
    assert_eq!(runtime.get_global("removed"), Some(Value::Number(1)));
    assert!(runtime
        .get_global("d")
        .unwrap()
        .as_dict()
        .unwrap()
        .borrow()
        .get_str("a")
        .is_none());
    assert_eq!(runtime.get_global("getb"), Some(Value::Number(9)));
    let blob = runtime.get_global("b").unwrap();
    assert_eq!(blob.as_blob().unwrap().borrow().as_slice(), &[2, 1]);
    assert_eq!(runtime.get_global("b0"), Some(Value::Number(2)));
}

#[test]
fn substitute_flags_and_errors() {
    assert_eq!(
        eval(&mut VimRuntime::new(), "substitute('ABC', 'b', 'x', 'i')"),
        Value::string("AxC")
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "substitute('abc', 'z', 'x', '')"),
        Value::string("abc")
    );
    let error = eval_expression(
        b"substitute('abc', 'b', 'x', 'z')",
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, Some(475));
    let error = eval_expression(
        b"substitute('abc', 'b', '\\=x', '')",
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("not supported"));
}

#[test]
fn printf_errors_and_unsigned() {
    let error = eval_expression(
        b"printf('%d')",
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, Some(767));
    let error = eval_expression(
        b"printf('%q', 1)",
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, Some(767));
    assert_eq!(
        eval(&mut VimRuntime::new(), "printf('%u', -1)"),
        Value::string("18446744073709551615")
    );
}

#[test]
fn str2nr_base_and_range_stride_errors() {
    let error = eval_expression(
        b"str2nr('10', 3)",
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, Some(474));
    let error = eval_expression(
        b"range(1, 3, 0)",
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, Some(726));
    assert_eq!(
        eval(&mut VimRuntime::new(), "range(4)"),
        eval(&mut VimRuntime::new(), "[0,1,2,3]")
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "range(3, 1)"),
        eval(&mut VimRuntime::new(), "[3,2,1]")
    );
}

#[test]
fn nested_try_finally_ordering() {
    let runtime = run(
        "let g:log = []\ntry\n  try\n    throw 'inner'\n  finally\n    call add(g:log, 'inner-finally')\n  endtry\ncatch /inner/\n  call add(g:log, 'outer-catch')\nfinally\n  call add(g:log, 'outer-finally')\nendtry\n",
    );
    assert_eq!(
        items(&runtime.get_global("log").unwrap()),
        vec!["inner-finally", "outer-catch", "outer-finally"]
    );
}

#[test]
fn v_predefined_values() {
    let mut runtime = VimRuntime::new();
    assert_eq!(runtime.get_vvar("version"), Some(Value::Number(902)));
    assert_eq!(
        runtime.get_vvar("versionlong"),
        Some(Value::Number(9021338))
    );
    assert_eq!(runtime.get_vvar("vim_did_enter"), Some(Value::Number(1)));
    assert_eq!(
        runtime.get_vvar("imagebackend"),
        Some(Value::string("none"))
    );
    assert_eq!(runtime.get_vvar("count"), Some(Value::Number(0)));
    assert_eq!(runtime.get_vvar("count1"), Some(Value::Number(1)));
    assert_eq!(runtime.get_vvar("register"), Some(Value::string("\"")));
    assert!(runtime.get_vvar("argv").unwrap().is_list());
    assert!(runtime.get_vvar("completed_item").unwrap().is_dict());
    assert!(matches!(
        runtime.get_vvar("exiting"),
        Some(Value::Special(SpecialValue::Null))
    ));
    let mut host = BasicHost::new();
    execute_script(
        b"let v:errmsg = 'custom'",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(runtime.get_vvar("errmsg"), Some(Value::string("custom")));
}

#[test]
fn locking_containers_prevents_element_changes() {
    let runtime = run(
        "let g:l = [1]\nlockvar g:l\nlet g:err = 0\ntry\n  let g:l[0] = 2\ncatch /E741/\n  let g:err = 1\nendtry\nunlockvar g:l\nlet g:l[0] = 2\nlet g:l0 = g:l[0]\n",
    );
    assert_eq!(runtime.get_global("err"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("l0"), Some(Value::Number(2)));
}

#[test]
fn locking_global_inside_function_and_script_scope() {
    let runtime = run(
        "let g:x = 1\nfunction! LockIt()\n  lockvar g:x\n  return 1\nendfunction\ncall LockIt()\nlet g:err = 0\ntry\n  let g:x = 2\ncatch /E741/\n  let g:err = 1\nendtry\nlet s:y = 1\nlockvar s:y\nlet g:err2 = 0\ntry\n  let s:y = 2\ncatch /E741/\n  let g:err2 = 1\nendtry\n",
    );
    assert_eq!(runtime.get_global("err"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("err2"), Some(Value::Number(1)));
}

#[test]
fn nul_safe_dictionary_keys_and_blob_assignment() {
    let mut host = BasicHost::new();
    let mut runtime = VimRuntime::new();
    let value = eval_expression(br#"{"a\x00b": 1}"#, &mut runtime, &mut host).unwrap();
    let key = VimString::from(b"a\x00b".as_slice());
    assert_eq!(
        value.as_dict().unwrap().borrow().get(&key),
        Some(&Value::Number(1))
    );
    execute_script(
        b"let g:b = 0z0102\nlet g:b[0] = 255\nlet g:b[0:0] = [9]\nlet g:bv = g:b[0]\nlet g:bs = string(g:b)\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(runtime.get_global("bv"), Some(Value::Number(9)));
    assert_eq!(runtime.get_global("bs"), Some(Value::string("0z0902")));
}

#[test]
fn destructuring_errors() {
    assert_eq!(fail("let [g:a, g:b] = [1]\n").code, Some(687));
    assert_eq!(fail("let [g:a] = 1\n").code, Some(687));
    assert_eq!(fail("for [a, b] in [[1]]\nendfor\n").code, Some(687));
    assert_eq!(fail("for x in {}\nendfor\n").code, Some(714));
}

#[test]
fn multibyte_for_iteration() {
    let runtime = run("let g:chars = []\nfor c in 'éx'\n  call add(g:chars, c)\nendfor\n");
    assert_eq!(items(&runtime.get_global("chars").unwrap()), vec!["é", "x"]);
}

#[test]
fn cyclic_values_have_safe_repr() {
    let runtime = run(
        "let g:l = []\ncall add(g:l, g:l)\nlet g:ls = string(g:l)\nlet g:d = {}\nlet g:d.self = g:d\nlet g:ds = string(g:d)\n",
    );
    assert_eq!(runtime.get_global("ls"), Some(Value::string("[[...]]")));
    assert_eq!(
        runtime.get_global("ds"),
        Some(Value::string("{'self': {...}}"))
    );
}

#[test]
fn collect_cycles_preserves_reachable_objects() {
    let mut runtime = run("let g:l = []\ncall add(g:l, g:l)\n");
    collect_cycles();
    let length = eval(&mut runtime, "len(g:l)");
    assert_eq!(length, Value::Number(1));
    runtime.remove_global("l");
    drop(runtime);
    collect_cycles();
    assert_eq!(live_object_count(), 0);
}

#[test]
fn call_function_public_api() {
    let mut runtime = run("function! Add(a, b)\n  return a:a + a:b\nendfunction\n");
    let mut host = BasicHost::new();
    let value = call_function(
        "Add",
        vec![Value::Number(2), Value::Number(3)],
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(value, Value::Number(5));
    let value = call_function("len", vec![Value::string("abcd")], &mut runtime, &mut host).unwrap();
    assert_eq!(value, Value::Number(4));
    let error = call_function("Missing", vec![], &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(117));
}

#[test]
fn uncaught_echoerr_and_source_read_error() {
    let error = fail("echoerr 'boom'\n");
    assert!(error.exception.as_deref().unwrap_or("").contains("echoerr"));
    let error = fail("source /definitely/not/a/real/rvim-script.vim\n");
    assert_eq!(error.code, Some(484));
}

#[test]
fn plain_option_access_and_exists() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    host.options.insert(
        (OptionScope::Default, "ignorecase".to_string()),
        Value::Number(1),
    );
    assert_eq!(
        eval_expression(b"exists('&ignorecase')", &mut runtime, &mut host).unwrap(),
        Value::Number(1)
    );
    assert_eq!(
        eval_expression(b"exists('&nope')", &mut runtime, &mut host).unwrap(),
        Value::Number(0)
    );
    assert_eq!(
        eval_expression(b"'A' == 'a'", &mut runtime, &mut host).unwrap(),
        Value::Number(1)
    );
}

#[test]
fn execute_builtin_error_is_catchable() {
    let mut runtime = VimRuntime::new();
    let mut host = EmptyHost;
    execute_script(
        b"try\n  call execute('no_such_command')\ncatch /E492/\n  let g:caught = 1\nendtry\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert!(runtime
        .get_vvar("errmsg")
        .unwrap()
        .echo_string()
        .contains("E492"));
}

#[test]
fn nested_execute_capture() {
    let runtime =
        run("let g:inner = execute('echo \"deep\"')\nlet g:outer = execute('let g:ignored = 1')\n");
    assert_eq!(runtime.get_global("inner"), Some(Value::string("deep\n")));
    assert_eq!(runtime.get_global("outer"), Some(Value::string("")));
    assert_eq!(runtime.get_global("ignored"), Some(Value::Number(1)));
}

#[test]
fn vim9_script_header_is_rejected_by_the_legacy_engine() {
    // The editor script loader routes real Vim9 files to the Vim9 evaluator
    // before they reach this engine; a bare header must still be reported.
    let error = fail("vim9script\n");
    assert!(error.to_string().contains("Vim9"));
}

#[test]
fn scriptversion_and_modifier_abbreviations() {
    let (runtime, host) = {
        let mut runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        execute_script(
            b"scriptversion 4\nsil echo 'quiet'\nsil! echo 'quiet2'\nverb echo 'loud'\n",
            "test.vim",
            &mut runtime,
            &mut host,
        )
        .unwrap();
        (runtime, host)
    };
    assert_eq!(host.messages, vec!["loud".to_string()]);
    let _ = runtime;
    assert_eq!(fail("scriptversion 5\n").code, Some(481));
}

#[test]
fn dict_function_requires_a_dictionary() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    execute_script(
        b"function! Greet() dict\n  return self.name\nendfunction\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    let error = eval_expression(b"Greet()", &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(725));
}

#[test]
fn utf8_character_builtins() {
    assert_eq!(
        eval(&mut VimRuntime::new(), "nr2char(233)")
            .as_str()
            .unwrap()
            .as_bytes(),
        "é".as_bytes()
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "char2nr('é')"),
        Value::Number(233)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "printf('%c', 233)"),
        Value::string("é")
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "strpart('héllo', 1, 2, 1)"),
        Value::string("él")
    );
}

#[test]
fn match_functions_follow_ignorecase() {
    let runtime =
        run("let &ignorecase = 1\nlet g:m = matchstr('ABC', 'b')\nlet g:i = match('ABC', 'b')\n");
    assert_eq!(runtime.get_global("m"), Some(Value::string("B")));
    assert_eq!(runtime.get_global("i"), Some(Value::Number(1)));
}

#[test]
fn unsupported_pattern_and_replacement_features_error() {
    assert_eq!(
        eval_expression(
            br#"'foobar' =~ 'foo\zsbar'"#,
            &mut VimRuntime::new(),
            &mut BasicHost::new(),
        )
        .unwrap()
        .echo_string(),
        "1"
    );
    assert_eq!(
        eval_expression(
            br#"matchstr('foobar', 'foo\zsbar')"#,
            &mut VimRuntime::new(),
            &mut BasicHost::new(),
        )
        .unwrap()
        .echo_string(),
        "bar"
    );
    let error = eval_expression(
        br#"substitute('abc', 'b', '\u', '')"#,
        &mut VimRuntime::new(),
        &mut BasicHost::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("not supported"));
}

#[test]
fn extend_dictionary_keep_and_force() {
    let runtime = run(
        "let g:keep = {'a': 1}\ncall extend(g:keep, {'a': 2, 'b': 3}, 'keep')\nlet g:ka = g:keep.a\nlet g:kb = g:keep.b\nlet g:force = {'a': 1}\ncall extend(g:force, {'a': 2}, 'force')\nlet g:fa = g:force.a\n",
    );
    assert_eq!(runtime.get_global("ka"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("kb"), Some(Value::Number(3)));
    assert_eq!(runtime.get_global("fa"), Some(Value::Number(2)));
}

#[test]
fn register_and_patch_exists() {
    assert_eq!(
        eval(&mut VimRuntime::new(), "exists('@a')"),
        Value::Number(1)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "has('patch-9.2.1338')"),
        Value::Number(1)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "has('no_such_feature')"),
        Value::Number(0)
    );
}

#[test]
fn common_vim_error_catch_pattern_works() {
    let runtime = run(
        "let g:caught = 0\ntry\n  let g:x = no_such_variable\ncatch /^Vim\\%((\\a\\+)\\)\\=:E/\n  let g:caught = 1\nendtry\n",
    );
    assert_eq!(runtime.get_global("caught"), Some(Value::Number(1)));
}

#[test]
fn silent_bang_swallows_errors() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    execute_script(
        b"silent! call NoSuchFunction()\nlet g:after = 1\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(runtime.get_global("after"), Some(Value::Number(1)));
}

#[test]
fn function_script_alias_resolves() {
    let runtime = run(
        "function! <SID>Hello()\n  return 'hi'\nendfunction\nlet g:r = function('<script>Hello')()\n",
    );
    assert_eq!(runtime.get_global("r"), Some(Value::string("hi")));
}

#[test]
fn runtime_convenience_methods() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    runtime
        .execute(
            b"function! Twice(x)\n  return a:x * 2\nendfunction\n",
            "conv.vim",
            &mut host,
        )
        .unwrap();
    let value = runtime.eval(b"Twice(21)", &mut host).unwrap();
    assert_eq!(value, Value::Number(42));
    let value = runtime
        .call("Twice", vec![Value::Number(4)], &mut host)
        .unwrap();
    assert_eq!(value, Value::Number(8));
}

#[test]
fn getline_and_line_specials() {
    let (mut runtime, mut host) = {
        let runtime = VimRuntime::new();
        let mut host = BasicHost::new();
        host.lines = vec![
            VimString::from("one"),
            VimString::from("two"),
            VimString::from("three"),
        ];
        host.current_line = 2;
        (runtime, host)
    };
    let mut evaluate =
        |input: &str| eval_expression(input.as_bytes(), &mut runtime, &mut host).unwrap();
    assert_eq!(evaluate("getline('.')"), Value::string("two"));
    assert_eq!(evaluate("getline('$')"), Value::string("three"));
    assert_eq!(
        evaluate("getline(1, '$')"),
        eval_fresh_for_test("['one', 'two', 'three']")
    );
    assert_eq!(evaluate("line('.')"), Value::Number(2));
    assert_eq!(evaluate("line('$')"), Value::Number(3));
    assert_eq!(evaluate("setline('.', 'TWO')"), Value::Number(0));
    assert_eq!(host.lines[1].to_string_lossy(), "TWO");
}

fn eval_fresh_for_test(input: &str) -> Value {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    eval_expression(input.as_bytes(), &mut runtime, &mut host).unwrap()
}

#[test]
fn patch_feature_comparison() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    let mut evaluate =
        |input: &str| eval_expression(input.as_bytes(), &mut runtime, &mut host).unwrap();
    assert_eq!(evaluate("has('patch-9.2.1')"), Value::Number(1));
    assert_eq!(evaluate("has('patch-9.2.1338')"), Value::Number(1));
    assert_eq!(evaluate("has('patch-9.3.1')"), Value::Number(0));
}

#[test]
fn parse_errors_carry_line_and_script() {
    let error = fail("let g:a = 1\nlet g:b = 2\nlet g:c = 1 +\n");
    assert_eq!(error.code, Some(15));
    let throwpoint = error.throwpoint.unwrap_or_default();
    assert!(
        throwpoint.contains("line 3"),
        "unexpected throwpoint: {throwpoint}"
    );
    assert!(
        throwpoint.contains("test.vim"),
        "unexpected throwpoint: {throwpoint}"
    );
}

#[test]
fn function_body_parse_error_throwpoint_names_function() {
    let error = fail("function! Bad()\n  if 1\nendfunction\ncall Bad()\n");
    let throwpoint = error.throwpoint.unwrap_or_default();
    assert!(
        throwpoint.contains("function Bad"),
        "unexpected throwpoint: {throwpoint}"
    );
    assert!(
        throwpoint.contains("line 2"),
        "unexpected throwpoint: {throwpoint}"
    );
}

#[test]
fn whole_global_dictionary_is_live() {
    let runtime = run(
        "let g:x = 1\nlet g:d = g:\nlet g:d.y = 2\ncall extend(g:, {'z': 3})\nlet g:keys = sort(keys(g:))\n",
    );
    assert_eq!(runtime.get_global("y"), Some(Value::Number(2)));
    assert_eq!(runtime.get_global("z"), Some(Value::Number(3)));
    assert_eq!(
        runtime
            .get_global("keys")
            .unwrap()
            .as_list()
            .unwrap()
            .borrow()
            .items
            .iter()
            .map(Value::echo_string)
            .collect::<Vec<_>>(),
        vec!["d", "x", "y", "z"]
    );
}

#[test]
fn local_scope_dictionary_is_live() {
    let runtime = run(
        "function! F()\n  let x = 1\n  let d = l:\n  let d.y = 2\n  return y\nendfunction\nlet g:r = F()\n",
    );
    assert_eq!(runtime.get_global("r"), Some(Value::Number(2)));
}

#[test]
fn hash_dictionary_literal_control_keys_and_group_anchors() {
    assert_eq!(
        eval(&mut VimRuntime::new(), "#{a: 1, b: 2}.b"),
        Value::Number(2)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), r#""\<C-A>""#)
            .as_str()
            .unwrap()
            .as_bytes(),
        &[1u8]
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), r#"'abc' =~ '\(^abc\)'"#),
        Value::Number(1)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), r#"'xabc' =~ '\(^abc\)'"#),
        Value::Number(0)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), r#"'abc' =~ '\(bc\)$'"#),
        Value::Number(1)
    );
}

#[test]
fn method_calls_to_script_local_functions() {
    let runtime = run(
        "function! s:Double(x)\n  return a:x * 2\nendfunction\nlet g:r1 = 4->s:Double()\nlet g:r2 = [3, 1]->sort()\n",
    );
    assert_eq!(runtime.get_global("r1"), Some(Value::Number(8)));
    assert_eq!(
        runtime.get_global("r2"),
        Some(Value::list(vec![Value::Number(1), Value::Number(3)]))
    );
}

#[test]
fn null_and_none_identity_and_uncaught_function_throw() {
    assert_eq!(
        eval(&mut VimRuntime::new(), "v:null is v:null"),
        Value::Number(1)
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "v:null is v:none"),
        Value::Number(0)
    );
    let mut runtime = run("function! Boom()\n  throw 'bang'\nendfunction\n");
    let mut host = BasicHost::new();
    let error = eval_expression(b"Boom()", &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(605));
    assert_eq!(error.exception.as_deref(), Some("bang"));
}

#[test]
fn unicode_case_mapping_and_env_existence() {
    assert_eq!(
        eval(&mut VimRuntime::new(), "tolower('É')"),
        Value::string("é")
    );
    assert_eq!(
        eval(&mut VimRuntime::new(), "toupper('é')"),
        Value::string("É")
    );
    std::env::set_var("RVIM_VIML_EMPTY_ENV", "");
    assert_eq!(
        eval(&mut VimRuntime::new(), "exists('$RVIM_VIML_EMPTY_ENV')"),
        Value::Number(1)
    );
    std::env::remove_var("RVIM_VIML_EMPTY_ENV");
}

#[test]
fn exists_consults_host_for_ex_commands() {
    struct CommandHost;
    impl VimHost for CommandHost {
        fn has_command(&mut self, name: &str) -> bool {
            name == "set"
        }
    }
    let mut runtime = VimRuntime::new();
    let mut host = CommandHost;
    assert_eq!(
        eval_expression(b"exists(':set')", &mut runtime, &mut host).unwrap(),
        Value::Number(1)
    );
    assert_eq!(
        eval_expression(b"exists(':echo')", &mut runtime, &mut host).unwrap(),
        Value::Number(1)
    );
    assert_eq!(
        eval_expression(b"exists(':bogus')", &mut runtime, &mut host).unwrap(),
        Value::Number(0)
    );
}

#[test]
fn sort_comparator_receives_dictionary() {
    let runtime = run(
        "function! Cmp(a, b) dict\n  return self.reverse ? a:b - a:a : a:a - a:b\nendfunction\nlet g:opts = {'reverse': 1}\nlet g:r = sort([1, 3, 2], function('Cmp'), g:opts)\n",
    );
    assert_eq!(
        runtime.get_global("r"),
        Some(Value::list(vec![
            Value::Number(3),
            Value::Number(2),
            Value::Number(1)
        ]))
    );
}

#[test]
fn host_call_function_hook_and_global_option_fallback() {
    struct HostFunctions;
    impl VimHost for HostFunctions {
        fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Option<Value>> {
            if name == "HostAnswer" {
                assert!(args.is_empty());
                Ok(Some(Value::Number(42)))
            } else {
                Ok(None)
            }
        }
        fn get_option(&mut self, scope: OptionScope, name: &str) -> Result<Value> {
            if name == "ignorecase" && scope == OptionScope::Global {
                Ok(Value::Number(1))
            } else {
                Err(VimError::new(201, "unknown option"))
            }
        }
    }
    let mut runtime = VimRuntime::new();
    let mut host = HostFunctions;
    assert_eq!(
        eval_expression(b"HostAnswer()", &mut runtime, &mut host).unwrap(),
        Value::Number(42)
    );
    assert_eq!(
        eval_expression(b"'A' == 'a'", &mut runtime, &mut host).unwrap(),
        Value::Number(1)
    );
    assert_eq!(
        eval_expression(b"matchstr('ABC', 'b')", &mut runtime, &mut host).unwrap(),
        Value::string("B")
    );
}

#[test]
fn whole_buffer_scope_dictionary_via_host() {
    let mut runtime = VimRuntime::new();
    let mut host = BasicHost::new();
    execute_script(
        b"let b:one = 1\nlet b:two = 2\nlet g:keys = sort(keys(b:))\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        runtime
            .get_global("keys")
            .unwrap()
            .as_list()
            .unwrap()
            .borrow()
            .items
            .iter()
            .map(Value::echo_string)
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );
}
