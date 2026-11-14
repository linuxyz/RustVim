//! Crate-level integration tests for the interpreter.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::*;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn runtime_and_host() -> (VimRuntime, BasicHost) {
    (VimRuntime::new(), BasicHost::new())
}

fn eval(runtime: &mut VimRuntime, host: &mut BasicHost, input: &str) -> Result<Value> {
    eval_expression(input.as_bytes(), runtime, host)
}

fn eval_fresh(input: &str) -> Value {
    let (mut runtime, mut host) = runtime_and_host();
    eval(&mut runtime, &mut host, input).unwrap()
}

fn script(source: &str) -> VimRuntime {
    let (mut runtime, mut host) = runtime_and_host();
    execute_script(source.as_bytes(), "test.vim", &mut runtime, &mut host).unwrap();
    runtime
}

fn script_and_host(source: &str) -> (VimRuntime, BasicHost) {
    let (mut runtime, mut host) = runtime_and_host();
    execute_script(source.as_bytes(), "test.vim", &mut runtime, &mut host).unwrap();
    (runtime, host)
}

fn script_error(source: &str) -> VimError {
    let (mut runtime, mut host) = runtime_and_host();
    execute_script(source.as_bytes(), "test.vim", &mut runtime, &mut host).unwrap_err()
}

fn eval_value(runtime: &mut VimRuntime, host: &mut BasicHost, input: &str) -> Value {
    eval(runtime, host, input).unwrap()
}

fn list_strings(value: &Value) -> Vec<String> {
    value
        .as_list()
        .unwrap()
        .borrow()
        .items
        .iter()
        .map(Value::echo_string)
        .collect()
}

fn temp_script(name: &str, source: &str) -> std::path::PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "rvim_viml_{}_{}_{}.vim",
        std::process::id(),
        counter,
        name
    ));
    std::fs::write(&path, source).unwrap();
    path
}

// ---------------------------------------------------------------- values ---

#[test]
fn numbers_floats_and_strings() {
    assert_eq!(eval_fresh("0x10 + 0b10 + 0o10 + 010"), Value::Number(34));
    assert_eq!(eval_fresh("9223372036854775807"), Value::Number(i64::MAX));
    assert_eq!(eval_fresh("1.5e2"), Value::Float(150.0));
    assert_eq!(eval_fresh(r#"'it''s'"#), Value::string("it's"));
    assert_eq!(eval_fresh(r#""a\nb""#), Value::string("a\nb"));
    assert_eq!(
        eval_fresh(r#""\x41\u00e9""#).as_str().unwrap().as_bytes(),
        b"A\xc3\xa9"
    );
    assert_eq!(
        eval_fresh(r#""a\<CR>b""#).as_str().unwrap().as_bytes(),
        b"a\rb"
    );
    assert_eq!(eval_fresh(r#""a\x00b""#).as_str().unwrap().len(), 3);
}

#[test]
fn lists_dicts_blobs_and_types() {
    assert_eq!(eval_fresh("[1, 'x', 2.5]->len()"), Value::Number(3));
    assert_eq!(eval_fresh("len({'a': 1, 'b': 2})"), Value::Number(2));
    assert_eq!(eval_fresh("type(0)"), Value::Number(VIM_T_NUMBER));
    assert_eq!(eval_fresh("type('')"), Value::Number(VIM_T_STRING));
    assert_eq!(eval_fresh("type([])"), Value::Number(VIM_T_LIST));
    assert_eq!(eval_fresh("type({})"), Value::Number(VIM_T_DICT));
    assert_eq!(eval_fresh("type(1.0)"), Value::Number(VIM_T_FLOAT));
    assert_eq!(eval_fresh("type(v:true)"), Value::Number(VIM_T_BOOL));
    assert_eq!(eval_fresh("type(v:null)"), Value::Number(VIM_T_NONE));
    assert_eq!(eval_fresh("type(0z00)"), Value::Number(VIM_T_BLOB));
    assert_eq!(
        eval_fresh("type(function('len'))"),
        Value::Number(VIM_T_FUNC)
    );
    assert_eq!(eval_fresh("typename(v:true)"), Value::string("bool"));
    assert_eq!(eval_fresh("0zDEADBEEF->len()"), Value::Number(4));
    assert_eq!(eval_fresh("0z010203[1]"), Value::Number(2));
    assert_eq!(eval_fresh("0z010203[1:2]"), eval_fresh("0z0203"));
}

#[test]
fn indexing_and_slicing() {
    assert_eq!(eval_fresh("[1, 2, 3][-1]"), Value::Number(3));
    let error =
        eval_expression(b"[1,2,3][5]", &mut VimRuntime::new(), &mut BasicHost::new()).unwrap_err();
    assert_eq!(error.code, Some(684));
    assert_eq!(eval_fresh("'abcd'[-2]"), Value::string("c"));
    assert_eq!(eval_fresh("'abcd'[1:2]"), Value::string("bc"));
    assert_eq!(eval_fresh("'abcd'[9]"), Value::string(""));
    assert_eq!(eval_fresh("[1,2,3,4][1:-2]"), eval_fresh("[2,3]"));
    assert_eq!(eval_fresh("[1,2,3][:]"), eval_fresh("[1,2,3]"));

    let runtime = script(
        "let g:l = [1, 2, 3, 4]\nlet g:l[1:2] = [9]\nlet g:l[0] = 7\nlet g:d = {'k': 1}\nlet g:d.k = 2\nlet g:d['j'] = 3\n",
    );
    assert_eq!(runtime.get_global("l"), Some(eval_fresh("[7, 9, 4]")));
    let dict = runtime.get_global("d").unwrap();
    assert_eq!(
        dict.as_dict().unwrap().borrow().get_str("k"),
        Some(&Value::Number(2))
    );
    assert_eq!(
        dict.as_dict().unwrap().borrow().get_str("j"),
        Some(&Value::Number(3))
    );
}

#[test]
fn aliasing_copy_and_deepcopy() {
    let mut runtime = script(
        "let g:a = [1]\nlet g:b = g:a\ncall add(g:b, 2)\nlet g:c = copy(g:a)\ncall add(g:c, 3)\nlet g:cyc = []\ncall add(g:cyc, g:cyc)\nlet g:deep = deepcopy(g:cyc)\n",
    );
    assert_eq!(
        list_strings(&runtime.get_global("a").unwrap()),
        vec!["1", "2"]
    );
    assert_eq!(
        list_strings(&runtime.get_global("c").unwrap()),
        vec!["1", "2", "3"]
    );
    let deep = runtime.get_global("deep").unwrap();
    let deep_inner = deep.as_list().unwrap().borrow().items[0].clone();
    assert!(deep_inner.as_list().unwrap().borrow().items[0]
        .as_list()
        .is_some());
    assert_eq!(deep.object_id(), deep_inner.object_id());
    // break the explicit cycle before the runtime is dropped (not required, but
    // keeps this test's live object count deterministic)
    runtime.remove_global("cyc");
    collect_cycles();
}

#[test]
fn string_and_float_representations() {
    assert_eq!(eval_fresh("string(1.0)"), Value::string("1.0"));
    assert_eq!(eval_fresh("string(1.5)"), Value::string("1.5"));
    assert_eq!(eval_fresh("string(0.1)"), Value::string("0.1"));
    assert_eq!(eval_fresh("string('a')"), Value::string("'a'"));
    assert_eq!(eval_fresh("string([1, 'a'])"), Value::string("[1, 'a']"));
    assert_eq!(eval_fresh("string(v:true)"), Value::string("v:true"));
    assert_eq!(eval_fresh("string(v:null)"), Value::string("v:null"));
    assert_eq!(
        eval_fresh("string(function('len'))"),
        Value::string("function('len')")
    );
}

// -------------------------------------------------------------- operators ---

#[test]
fn arithmetic_and_coercion() {
    assert_eq!(eval_fresh("1 + 2 * 3"), Value::Number(7));
    assert_eq!(eval_fresh("7 / 2"), Value::Number(3));
    assert_eq!(eval_fresh("-7 / 2"), Value::Number(-3));
    assert_eq!(eval_fresh("7 % 3"), Value::Number(1));
    assert_eq!(eval_fresh("2.5 * 2"), Value::Float(5.0));
    assert_eq!(eval_fresh("'5' + 1"), Value::Number(6));
    assert_eq!(eval_fresh("'5' . 1"), Value::string("51"));
    assert_eq!(eval_fresh("[1] + [2]"), eval_fresh("[1, 2]"));
    assert_eq!(eval_fresh("0z01 + 0z02"), eval_fresh("0z0102"));
    let error =
        eval_expression(b"1 / 0", &mut VimRuntime::new(), &mut BasicHost::new()).unwrap_err();
    assert_eq!(error.code, Some(805));
}

#[test]
fn comparisons_matching_and_identity() {
    assert_eq!(eval_fresh("1 == 1.0"), Value::Number(1));
    assert_eq!(eval_fresh("'A' == 'a'"), Value::Number(0));
    assert_eq!(eval_fresh("'A' ==? 'a'"), Value::Number(1));
    assert_eq!(eval_fresh("'A' ==# 'a'"), Value::Number(0));
    assert_eq!(eval_fresh("'abc' =~ '^a'"), Value::Number(1));
    assert_eq!(eval_fresh("'abc' !~ '^b'"), Value::Number(1));
    assert_eq!(eval_fresh("1 =~ '\\d'"), Value::Number(1));
    assert_eq!(eval_fresh("[1, 2] == [1, 2]"), Value::Number(1));
    assert_eq!(eval_fresh("{'a': 1} == {'a': 1}"), Value::Number(1));

    let runtime = script(
        "let g:l = [1]\nlet g:m = g:l\nlet g:same = g:l is g:m\nlet g:different = [1] is [1]\n",
    );
    assert_eq!(runtime.get_global("same"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("different"), Some(Value::Number(0)));
}

#[test]
fn ternary_and_short_circuit() {
    assert_eq!(eval_fresh("1 ? 'a' : 'b'"), Value::string("a"));
    assert_eq!(eval_fresh("0 ? 'a' : 'b'"), Value::string("b"));
    assert_eq!(eval_fresh("0 && NoSuchFunction()"), Value::Number(0));
    assert_eq!(eval_fresh("1 || NoSuchFunction()"), Value::Number(1));
    assert_eq!(eval_fresh("!0"), Value::Number(1));
}

// --------------------------------------------------------------- scopes ---

#[test]
fn variables_in_all_scopes() {
    let (mut runtime, mut host) = script_and_host(
        "let g:a = 1\nlet b:b = 2\nlet w:w = 3\nlet t:t = 4\nfunction! Scope(arg)\n  let l:l = 5\n  return g:a + b:b + w:w + t:t + l:l + a:arg\nendfunction\nlet g:r = Scope(6)\n",
    );
    assert_eq!(runtime.get_global("r"), Some(Value::Number(21)));
    assert_eq!(
        host.variables.get(&(VarScope::Buffer, "b".to_string())),
        Some(&Value::Number(2))
    );
    assert_eq!(
        host.variables.get(&(VarScope::Window, "w".to_string())),
        Some(&Value::Number(3))
    );
    assert_eq!(
        host.variables.get(&(VarScope::Tab, "t".to_string())),
        Some(&Value::Number(4))
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "b:b + w:w + t:t"),
        Value::Number(9)
    );
}

#[test]
fn unlet_destructuring_and_compounds() {
    let runtime = script(
        "let g:x = 1\nunlet g:x\nlet g:gone = exists('g:x')\nunlet! g:missing\nlet [g:a, g:b] = [1, 2]\nlet [g:c; g:rest] = [3, 4, 5]\nlet g:sum = 1\nlet g:sum += 2\nlet g:s = 'a'\nlet g:s .= 'b'\nlet g:l = [1]\nlet g:l += [2]\n",
    );
    assert_eq!(runtime.get_global("gone"), Some(Value::Number(0)));
    assert_eq!(runtime.get_global("a"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("b"), Some(Value::Number(2)));
    assert_eq!(runtime.get_global("c"), Some(Value::Number(3)));
    assert_eq!(
        list_strings(&runtime.get_global("rest").unwrap()),
        vec!["4", "5"]
    );
    assert_eq!(runtime.get_global("sum"), Some(Value::Number(3)));
    assert_eq!(runtime.get_global("s"), Some(Value::string("ab")));
    assert_eq!(
        list_strings(&runtime.get_global("l").unwrap()),
        vec!["1", "2"]
    );
}

#[test]
fn lockvar_and_const() {
    let runtime = script(
        "let g:x = 1\nlockvar g:x\nlet g:err = 0\ntry\n  let g:x = 2\ncatch /E741/\n  let g:err = 1\nendtry\nunlockvar g:x\nlet g:x = 3\nconst g:y = 10\nlet g:err2 = 0\ntry\n  let g:y = 11\ncatch /E741/\n  let g:err2 = 1\nendtry\n",
    );
    assert_eq!(runtime.get_global("err"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("x"), Some(Value::Number(3)));
    assert_eq!(runtime.get_global("err2"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("y"), Some(Value::Number(10)));
}

#[test]
fn whole_scope_dictionaries() {
    assert_eq!(
        eval_fresh("has_key(g:, 'does_not_exist')"),
        Value::Number(0)
    );
    let runtime = script("let g:x = 1\nlet g:has = has_key(g:, 'x')\nlet g:vnull = v:true\n");
    assert_eq!(runtime.get_global("has"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("vnull"), Some(Value::Bool(true)));
}

// ---------------------------------------------------------- control flow ---

#[test]
fn conditionals_and_loops() {
    let runtime = script(
        "let g:r = ''\nif 1\n  let g:r = 'if'\nelseif 1\n  let g:r = 'elseif'\nelse\n  let g:r = 'else'\nendif\nif 0\n  let g:r = 'bad'\nelse\n  let g:r .= 'else'\nendif\nlet g:i = 0\nwhile g:i < 3\n  let g:i += 1\nendwhile\nlet g:sum = 0\nfor x in range(1, 4)\n  if x == 2\n    continue\n  endif\n  if x == 4\n    break\n  endif\n  let g:sum += x\nendfor\nlet g:chars = []\nfor c in 'ab'\n  call add(g:chars, c)\nendfor\nlet g:blob = []\nfor b in 0z0102\n  call add(g:blob, b)\nendfor\n",
    );
    assert_eq!(runtime.get_global("r"), Some(Value::string("ifelse")));
    assert_eq!(runtime.get_global("i"), Some(Value::Number(3)));
    assert_eq!(runtime.get_global("sum"), Some(Value::Number(4)));
    assert_eq!(
        list_strings(&runtime.get_global("chars").unwrap()),
        vec!["a", "b"]
    );
    assert_eq!(
        list_strings(&runtime.get_global("blob").unwrap()),
        vec!["1", "2"]
    );
}

// ------------------------------------------------------------- functions ---

#[test]
fn functions_defaults_varargs_and_recursion() {
    let runtime = script(
        "function! F(a, b = 2, ...)\n  return [a:a, a:b, a:0, a:000]\nendfunction\nlet g:r1 = F(1)\nlet g:r2 = F(1, 3, 4, 5)\nfunction! G(a, b = a * 2)\n  return a:b\nendfunction\nlet g:r3 = G(3)\nfunction! Fact(n)\n  if a:n <= 1\n    return 1\n  endif\n  return a:n * Fact(a:n - 1)\nendfunction\nlet g:fact = Fact(5)\n",
    );
    assert_eq!(runtime.get_global("r1"), Some(eval_fresh("[1, 2, 0, []]")));
    assert_eq!(
        runtime.get_global("r2"),
        Some(eval_fresh("[1, 3, 2, [4, 5]]"))
    );
    assert_eq!(runtime.get_global("r3"), Some(Value::Number(6)));
    assert_eq!(runtime.get_global("fact"), Some(Value::Number(120)));
}

#[test]
fn function_arity_errors_and_delfunction() {
    let error = script_error("function! F(a)\n  return a:a\nendfunction\ncall F()\n");
    assert_eq!(error.code, Some(119));
    let error = script_error("function! F(a)\n  return a:a\nendfunction\ncall F(1, 2)\n");
    assert_eq!(error.code, Some(118));
    let runtime = script(
        "function! F()\n  return 1\nendfunction\ndelfunction F\nlet g:exists = exists('*F')\n",
    );
    assert_eq!(runtime.get_global("exists"), Some(Value::Number(0)));
}

#[test]
fn script_local_functions_and_sid() {
    let (mut runtime, mut host) = script_and_host(
        "function! s:Twice(x)\n  return a:x * 2\nendfunction\nfunction! <SID>Hello()\n  return 'hello'\nendfunction\nlet g:r1 = s:Twice(4)\nlet g:r2 = function('<SID>Hello')()\n",
    );
    assert_eq!(runtime.get_global("r1"), Some(Value::Number(8)));
    assert_eq!(runtime.get_global("r2"), Some(Value::string("hello")));
    assert_eq!(
        eval_value(&mut runtime, &mut host, "r1 + 1"),
        Value::Number(9)
    );

    // Two scripts with the same s: name keep separate dictionaries.
    let (mut runtime, mut host) = runtime_and_host();
    execute_script(
        b"let s:x = 10\nfunction! GetX()\n  return s:x\nendfunction\n",
        "one.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    execute_script(
        b"let s:x = 20\nfunction! GetY()\n  return s:x\nendfunction\n",
        "two.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        eval_value(&mut runtime, &mut host, "GetX() + GetY()"),
        Value::Number(30)
    );
}

#[test]
fn lambdas_closures_partials_and_methods() {
    let (mut runtime, mut host) = script_and_host(
        "let g:double = {x -> x * 2}\nlet g:r1 = g:double(5)\nfunction! MakeAdder(n)\n  return {x -> x + n}\nendfunction\nlet g:add10 = MakeAdder(10)\nlet g:r2 = g:add10(7)\nfunction! Add2(a, b)\n  return a:a + a:b\nendfunction\nlet g:add5 = function('Add2', [5])\nlet g:r3 = g:add5(3)\nlet g:r4 = [1, 2, 3]->len()\nlet g:r5 = call('len', [[1, 2]])\nlet g:r6 = function('len')([1, 2, 3, 4])\n",
    );
    assert_eq!(runtime.get_global("r1"), Some(Value::Number(10)));
    assert_eq!(runtime.get_global("r2"), Some(Value::Number(17)));
    assert_eq!(runtime.get_global("r3"), Some(Value::Number(8)));
    assert_eq!(runtime.get_global("r4"), Some(Value::Number(3)));
    assert_eq!(runtime.get_global("r5"), Some(Value::Number(2)));
    assert_eq!(runtime.get_global("r6"), Some(Value::Number(4)));
    assert_eq!(
        eval_value(&mut runtime, &mut host, "string(function('Add2', [5]))"),
        Value::string("function('Add2', [5])")
    );
}

#[test]
fn dictionary_functions_and_self() {
    let runtime = script(
        "function! Greet() dict\n  return 'hi ' . self.name\nendfunction\nlet g:d = {'name': 'bob', 'Greet': function('Greet')}\nlet g:r = g:d.Greet()\nlet g:r2 = get(g:d, 'name')\n",
    );
    assert_eq!(runtime.get_global("r"), Some(Value::string("hi bob")));
    assert_eq!(runtime.get_global("r2"), Some(Value::string("bob")));
}

// ------------------------------------------------------------ exceptions ---

#[test]
fn try_catch_finally_and_vvars() {
    let runtime = script(
        "let g:log = []\ntry\n  throw 'boom'\ncatch /boom/\n  call add(g:log, v:exception)\nfinally\n  call add(g:log, 'finally')\nendtry\nlet g:caught = 0\ntry\n  let g:x = no_such_variable\ncatch /E121/\n  let g:caught = 1\n  let g:ex = v:exception\n  let g:tp = v:throwpoint\nendtry\n",
    );
    assert_eq!(
        list_strings(&runtime.get_global("log").unwrap()),
        vec!["boom", "finally"]
    );
    assert_eq!(runtime.get_global("caught"), Some(Value::Number(1)));
    let exception = runtime.get_global("ex").unwrap();
    assert!(exception.echo_string().contains("E121"));
    assert!(!runtime.get_global("tp").unwrap().echo_string().is_empty());
}

#[test]
fn throwpoints_and_rethrow() {
    let runtime = script(
        "function! Boom()\n  return 1 / 0\nendfunction\nlet g:err = 0\ntry\n  call Boom()\ncatch /E805/\n  let g:err = 1\n  let g:tp = v:throwpoint\n  try\n    throw v:exception\n  catch /E805/\n    let g:rethrown = 1\n  endtry\nendtry\n",
    );
    assert_eq!(runtime.get_global("err"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("rethrown"), Some(Value::Number(1)));
    let throwpoint = runtime.get_global("tp").unwrap().echo_string();
    assert!(
        throwpoint.starts_with("function Boom"),
        "unexpected throwpoint: {throwpoint}"
    );
}

#[test]
fn return_through_finally_and_echoerr() {
    let runtime = script(
        "function! F()\n  try\n    return 1\n  finally\n    let g:ran = 1\n  endtry\nendfunction\nlet g:r = F()\nlet g:err = 0\ntry\n  echoerr 'bad news'\ncatch /bad news/\n  let g:err = 1\nendtry\n",
    );
    assert_eq!(runtime.get_global("r"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("ran"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("err"), Some(Value::Number(1)));
}

#[test]
fn uncaught_throw_is_an_error() {
    let (mut runtime, mut host) = runtime_and_host();
    let error =
        execute_script(b"throw 'uncaught'", "test.vim", &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(605));
    assert_eq!(error.exception.as_deref(), Some("uncaught"));
    assert!(error.message.contains("Exception not caught"));
}

// --------------------------------------------------------------- builtins ---

#[test]
fn builtin_string_functions() {
    assert_eq!(eval_fresh("str2nr('0x10')"), Value::Number(16));
    assert_eq!(eval_fresh("str2nr('10', 2)"), Value::Number(2));
    assert_eq!(eval_fresh("nr2str(255, 16)"), Value::string("ff"));
    assert_eq!(eval_fresh("str2float('1.5')"), Value::Float(1.5));
    assert_eq!(eval_fresh("float2nr(3.9)"), Value::Number(3));
    assert_eq!(eval_fresh("len('héllo')"), Value::Number(6));
    assert_eq!(eval_fresh("strchars('héllo')"), Value::Number(5));
    assert_eq!(eval_fresh("tolower('AbC')"), Value::string("abc"));
    assert_eq!(eval_fresh("toupper('AbC')"), Value::string("ABC"));
    assert_eq!(eval_fresh("repeat('ab', 3)"), Value::string("ababab"));
    assert_eq!(eval_fresh("stridx('abcabc', 'bc')"), Value::Number(1));
    assert_eq!(eval_fresh("strridx('abcabc', 'bc')"), Value::Number(4));
    assert_eq!(eval_fresh("strpart('abcdef', 2, 3)"), Value::string("cde"));
    assert_eq!(
        eval_fresh("strcharpart('héllo', 1, 2)"),
        Value::string("él")
    );
    assert_eq!(eval_fresh("char2nr('A')"), Value::Number(65));
    assert_eq!(eval_fresh("nr2char(65)"), Value::string("A"));
    assert_eq!(eval_fresh("empty('')"), Value::Number(1));
    assert_eq!(eval_fresh("empty([1])"), Value::Number(0));
}

#[test]
fn builtin_collection_functions() {
    let runtime = script(
        "let g:l = [1, 2]\ncall add(g:l, 3)\ncall insert(g:l, 0)\nlet g:removed = remove(g:l, 0)\ncall extend(g:l, [9], 1)\nlet g:r = reverse(copy(g:l))\nlet g:j = join(['a', 1], '-')\nlet g:sp = split('a,b,,c', ',')\nlet g:spk = split('a,b,,c', ',', 1)\nlet g:d = {'a': 1}\nlet g:keys = keys(g:d)\nlet g:values = values(g:d)\nlet g:hk = has_key(g:d, 'a')\nlet g:g1 = get(g:l, 99, 'none')\nlet g:idx = index([1, 2, 3], 2)\nlet g:cnt = count([1, 2, 1], 1)\nlet g:range = range(1, 3)\nlet g:sort1 = sort([10, 2])\nlet g:sort2 = sort([10, 2], 'n')\n",
    );
    assert_eq!(
        list_strings(&runtime.get_global("l").unwrap()),
        vec!["1", "9", "2", "3"]
    );
    assert_eq!(runtime.get_global("removed"), Some(Value::Number(0)));
    assert_eq!(
        list_strings(&runtime.get_global("r").unwrap()),
        vec!["3", "2", "9", "1"]
    );
    assert_eq!(runtime.get_global("j"), Some(Value::string("a-1")));
    assert_eq!(
        list_strings(&runtime.get_global("sp").unwrap()),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        list_strings(&runtime.get_global("spk").unwrap()),
        vec!["a", "b", "", "c"]
    );
    assert_eq!(
        list_strings(&runtime.get_global("keys").unwrap()),
        vec!["a"]
    );
    assert_eq!(
        list_strings(&runtime.get_global("values").unwrap()),
        vec!["1"]
    );
    assert_eq!(runtime.get_global("hk"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("g1"), Some(Value::string("none")));
    assert_eq!(runtime.get_global("idx"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("cnt"), Some(Value::Number(2)));
    assert_eq!(runtime.get_global("range"), Some(eval_fresh("[1, 2, 3]")));
    assert_eq!(eval_fresh("range(4)"), eval_fresh("[0, 1, 2, 3]"));
    assert_eq!(
        list_strings(&runtime.get_global("sort1").unwrap()),
        vec!["10", "2"]
    );
    assert_eq!(
        list_strings(&runtime.get_global("sort2").unwrap()),
        vec!["2", "10"]
    );
}

#[test]
fn builtin_map_filter_sort_comparator() {
    let runtime = script(
        "let g:doubled = map([1, 2, 3], 'v:val * 2')\nlet g:odds = filter([1, 2, 3, 4], 'v:val % 2')\nlet g:dict = {'a': 1}\ncall map(g:dict, 'v:val + 10')\nlet g:kept = {'a': 1, 'b': 2}\ncall filter(g:kept, 'v:val > 1')\nfunction! Rev(a, b)\n  return a:b - a:a\nendfunction\nlet g:rev = sort([1, 3, 2], function('Rev'))\nlet g:lambda = map([1, 2], {i, v -> v + 1})\nlet g:val_after = v:val\n",
    );
    assert_eq!(runtime.get_global("doubled"), Some(eval_fresh("[2, 4, 6]")));
    assert_eq!(runtime.get_global("odds"), Some(eval_fresh("[1, 3]")));
    assert_eq!(
        runtime
            .get_global("dict")
            .unwrap()
            .as_dict()
            .unwrap()
            .borrow()
            .get_str("a"),
        Some(&Value::Number(11))
    );
    assert_eq!(
        runtime
            .get_global("kept")
            .unwrap()
            .as_dict()
            .unwrap()
            .borrow()
            .get_str("b"),
        Some(&Value::Number(2))
    );
    assert!(runtime
        .get_global("kept")
        .unwrap()
        .as_dict()
        .unwrap()
        .borrow()
        .get_str("a")
        .is_none());
    assert_eq!(runtime.get_global("rev"), Some(eval_fresh("[3, 2, 1]")));
    assert_eq!(runtime.get_global("lambda"), Some(eval_fresh("[2, 3]")));
    assert_eq!(runtime.get_global("val_after"), Some(Value::Number(0)));
}

#[test]
fn builtin_printf_and_match_functions() {
    assert_eq!(eval_fresh("printf('%d-%s', 3, 'x')"), Value::string("3-x"));
    assert_eq!(eval_fresh("printf('%05d', 42)"), Value::string("00042"));
    assert_eq!(eval_fresh("printf('%-5s', 'x')"), Value::string("x    "));
    assert_eq!(eval_fresh("printf('%.2f', 1.234)"), Value::string("1.23"));
    assert_eq!(eval_fresh("printf('%x', 255)"), Value::string("ff"));
    assert_eq!(eval_fresh("printf('%%')"), Value::string("%"));
    assert_eq!(eval_fresh("matchstr('abc', 'a.c')"), Value::string("abc"));
    assert_eq!(eval_fresh("matchstr('abcd', 'b.d')"), Value::string("bcd"));
    assert_eq!(eval_fresh("match('abc', 'b')"), Value::Number(1));
    assert_eq!(eval_fresh("matchend('abc', 'b')"), Value::Number(2));
    assert_eq!(eval_fresh("matchstr('abc', 'z')"), Value::string(""));
    assert_eq!(
        eval_fresh("substitute('abcabc', 'b', 'X', 'g')"),
        Value::string("aXcaXc")
    );
    assert_eq!(
        eval_fresh(r#"substitute('ab', '\(a\)\(b\)', '\2\1', '')"#),
        Value::string("ba")
    );
    assert_eq!(eval_fresh("'abc' =~ '\\cABC'"), Value::Number(1));
}

#[test]
fn builtin_math_and_bits() {
    assert_eq!(eval_fresh("abs(-3)"), Value::Number(3));
    assert_eq!(eval_fresh("abs(-2.5)"), Value::Float(2.5));
    assert_eq!(eval_fresh("round(1.5)"), Value::Float(2.0));
    assert_eq!(eval_fresh("floor(1.9)"), Value::Float(1.0));
    assert_eq!(eval_fresh("ceil(1.1)"), Value::Float(2.0));
    assert_eq!(eval_fresh("sqrt(9)"), Value::Float(3.0));
    assert_eq!(eval_fresh("pow(2, 3)"), Value::Float(8.0));
    assert_eq!(eval_fresh("fmod(7, 3)"), Value::Float(1.0));
    assert_eq!(eval_fresh("and(6, 3)"), Value::Number(2));
    assert_eq!(eval_fresh("or(4, 1)"), Value::Number(5));
    assert_eq!(eval_fresh("xor(6, 3)"), Value::Number(5));
    assert_eq!(eval_fresh("invert(0)"), Value::Number(-1));
    assert_eq!(eval_fresh("min([3, 1, 2])"), Value::Number(1));
    assert_eq!(eval_fresh("max(3, 1, 2)"), Value::Number(3));
}

#[test]
fn builtin_exists_has_and_eval() {
    let mut runtime = script("let g:x = 1\n");
    let mut host = BasicHost::new();
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists('*len')"),
        Value::Number(1)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists('*NoSuchFunction')"),
        Value::Number(0)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists('g:x')"),
        Value::Number(1)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists('g:nope')"),
        Value::Number(0)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists('v:true')"),
        Value::Number(1)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists(':let')"),
        Value::Number(1)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "exists(':nonexistent')"),
        Value::Number(0)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "has('vim9script')"),
        Value::Number(1)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "has('rvim')"),
        Value::Number(1)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "eval('1 + 2')"),
        Value::Number(3)
    );
}

#[test]
fn builtin_getline_setline_and_line() {
    let (mut runtime, mut host) = runtime_and_host();
    host.lines = vec![VimString::from("one"), VimString::from("two")];
    host.current_line = 2;
    assert_eq!(
        eval_value(&mut runtime, &mut host, "getline(1)"),
        Value::string("one")
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "getline(1, 2)"),
        eval_fresh("['one', 'two']")
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "line()"),
        Value::Number(2)
    );
    assert_eq!(
        eval_value(&mut runtime, &mut host, "setline(1, 'ONE')"),
        Value::Number(0)
    );
    assert_eq!(host.lines[0].to_string_lossy(), "ONE");
    assert_eq!(
        eval_value(&mut runtime, &mut host, "setline(2, ['TWO', 'THREE'])"),
        Value::Number(0)
    );
    assert_eq!(host.lines[2].to_string_lossy(), "THREE");
}

#[test]
fn options_environment_registers_and_host_commands() {
    let (runtime, host) = script_and_host(
        "let &ignorecase = 1\nlet g:ic = 'A' == 'a'\nlet &l:shiftwidth = 4\nlet &g:shiftwidth = 8\nlet g:lsw = &l:shiftwidth\nlet g:gsw = &g:shiftwidth\nlet $RVIM_VIML_TEST_ENV = 'hello'\nlet g:env = $RVIM_VIML_TEST_ENV\nlet @a = 'reg'\nlet g:reg = @a\nset number\n",
    );
    assert_eq!(runtime.get_global("ic"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("lsw"), Some(Value::Number(4)));
    assert_eq!(runtime.get_global("gsw"), Some(Value::Number(8)));
    assert_eq!(runtime.get_global("env"), Some(Value::string("hello")));
    assert_eq!(runtime.get_global("reg"), Some(Value::string("reg")));
    assert_eq!(host.executed, vec!["set number".to_string()]);
    std::env::remove_var("RVIM_VIML_TEST_ENV");
}

// ----------------------------------------------------------------- execute ---

#[test]
fn echo_execute_and_eval_capture() {
    let (mut runtime, mut host) = runtime_and_host();
    execute_script(
        b"echo 'a' 'b'\nechomsg 'msg'\nechon 'x'\nexecute 'let g:x = 5'\nlet g:cap = execute('echo \"hi\"')\nlet g:cap2 = execute('echo \"a\" | echo \"b\"')\n",
        "test.vim",
        &mut runtime,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.messages,
        vec!["a b".to_string(), "msg".to_string(), "x".to_string()]
    );
    assert_eq!(runtime.get_global("x"), Some(Value::Number(5)));
    assert_eq!(runtime.get_global("cap"), Some(Value::string("hi\n")));
    assert_eq!(runtime.get_global("cap2"), Some(Value::string("a\nb\n")));
    assert_eq!(runtime.get_vvar("statusmsg"), Some(Value::string("msg")));
}

#[test]
fn execute_multiline_command_string() {
    let runtime = script("execute \"let g:a = 1\\nlet g:b = 2\"\n");
    assert_eq!(runtime.get_global("a"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("b"), Some(Value::Number(2)));
}

#[test]
fn line_continuation_comments_and_bars() {
    let runtime = script(
        "let g:x = 1 +\n      \\ 2\n\" full line comment\nlet g:y = 2 \" trailing comment\necho 'a' | let g:z = 3\n",
    );
    assert_eq!(runtime.get_global("x"), Some(Value::Number(3)));
    assert_eq!(runtime.get_global("y"), Some(Value::Number(2)));
    assert_eq!(runtime.get_global("z"), Some(Value::Number(3)));
}

#[test]
fn finish_and_scriptencoding() {
    let runtime = script("scriptencoding utf-8\nlet g:a = 1\nfinish\nlet g:a = 2\n");
    assert_eq!(runtime.get_global("a"), Some(Value::Number(1)));
    let error = script_error("scriptencoding latin1\n");
    assert!(error.to_string().contains("not supported"));
}

// ----------------------------------------------------------------- sourcing ---

#[test]
fn source_runs_nested_script_and_finish_is_local() {
    let path = temp_script("sourced", "let g:sourced = 1\nfinish\nlet g:sourced = 2\n");
    let source = format!(
        "let g:before = 1\nsource {}\nlet g:after = 1\n",
        path.display()
    );
    let runtime = script(&source);
    assert_eq!(runtime.get_global("before"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("sourced"), Some(Value::Number(1)));
    assert_eq!(runtime.get_global("after"), Some(Value::Number(1)));
    std::fs::remove_file(path).ok();
}

#[test]
fn source_error_reports_sourced_script() {
    let path = temp_script("bad", "let g:oops = no_such_variable\n");
    let source = format!("source {}\n", path.display());
    let error = script_error(&source);
    assert_eq!(error.code, Some(121));
    let source_name = path.to_string_lossy().into_owned();
    assert!(
        error
            .throwpoint
            .as_deref()
            .unwrap_or("")
            .contains(&source_name),
        "unexpected throwpoint: {:?}",
        error.throwpoint
    );
    std::fs::remove_file(path).ok();
}

#[test]
fn autoload_names_are_stubbed_with_explicit_error() {
    let error = script_error("call foo#bar#baz()\n");
    assert_eq!(error.code, Some(117));
    assert!(error.message.contains("autoload"));
}

// ------------------------------------------------------------------- errors ---

#[test]
fn common_errors_and_v_errmsg() {
    let (mut runtime, mut host) = runtime_and_host();
    let error = eval_expression(b"no_such_variable", &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(121));
    assert!(runtime
        .get_vvar("errmsg")
        .unwrap()
        .echo_string()
        .contains("E121"));

    let error = eval_expression(b"NoSuchFunction()", &mut runtime, &mut host).unwrap_err();
    assert_eq!(error.code, Some(117));

    let error = script_error("if 1\n  let g:x = 1\n");
    assert_eq!(error.code, Some(171));

    let error = script_error("function! Bad()\n  let g:x = 1\n");
    assert_eq!(error.code, Some(170));
}

#[test]
fn parse_error_positions_are_reported() {
    let error = eval_expression(b"1 +", &mut VimRuntime::new(), &mut BasicHost::new()).unwrap_err();
    assert_eq!(error.code, Some(15));
}

// ---------------------------------------------------------------------- GC ---

#[test]
fn cycle_collector_breaks_unreachable_cycles() {
    {
        let runtime =
            script("let g:l = []\ncall add(g:l, g:l)\nlet g:d = {'self': 0}\nlet g:d.self = g:d\n");
        let list = runtime.get_global("l").unwrap();
        let dict = runtime.get_global("d").unwrap();
        assert_eq!(
            list.as_list().unwrap().borrow().items[0].object_id(),
            list.object_id()
        );
        assert_eq!(
            dict.as_dict()
                .unwrap()
                .borrow()
                .get_str("self")
                .unwrap()
                .object_id(),
            dict.object_id()
        );
        assert!(live_object_count() > 0);
    }
    collect_cycles();
    assert_eq!(live_object_count(), 0);
}
