//! Unit tests for the Vim9 evaluator (Phase 2 base language).

use super::eval::run_script;

fn messages(source: &str) -> Vec<String> {
    run_script(source.as_bytes())
        .unwrap_or_else(|error| {
            panic!(
                "run failed: {}\n--- source ---\n{}",
                error.error_text(),
                source
            )
        })
        .messages
}

fn run_error(source: &str) -> String {
    match run_script(source.as_bytes()) {
        Ok(output) => panic!("expected an error, got messages: {:?}", output.messages),
        Err(error) => error.error_text(),
    }
}

#[test]
fn literals_and_formatting() {
    assert_eq!(
        messages(
            "vim9script
echomsg 'n=' .. 42 .. ' f=' .. 1.5 .. ' b=' .. true .. ' null=' .. null
echomsg string(0z0102)
echomsg string((9,))
"
        ),
        vec![
            "n=42 f=1.5 b=true null=null".to_string(),
            "0z0102".to_string(),
            "(9, )".to_string(),
        ]
    );
}

#[test]
fn operators_and_comparisons() {
    assert_eq!(
        messages(
            "vim9script
var a = 3
var b = 3
var list1 = [1, 2]
var list2 = [1, 2]
echomsg 'eq=' .. (a == b) .. ' lt=' .. ('abc' < 'abd')
echomsg 'case=' .. ('ABC' ==? 'abc') .. ' match=' .. ('abc123' =~ '\\d\\+')
echomsg 'listeq=' .. (list1 == list2) .. ' listis=' .. (list1 is list2) .. ' same=' .. (list1 is list1)
"
        ),
        vec![
            "eq=true lt=true".to_string(),
            "case=true match=true".to_string(),
            "listeq=true listis=false same=true".to_string(),
        ]
    );
}

#[test]
fn functions_defaults_varargs_and_recursion() {
    assert_eq!(
        messages(
            "vim9script
def Add(a: number, b: number): number
  return a + b
enddef
def Greet(name: string = 'world'): string
  return 'hello ' .. name
enddef
def Sum(...values: list<number>): number
  var total = 0
  for value in values
    total += value
  endfor
  return total
enddef
def Factorial(n: number): number
  return n <= 1 ? 1 : n * Factorial(n - 1)
enddef
echomsg 'add=' .. Add(2, 3) .. ' greet=' .. Greet()
echomsg 'sum=' .. Sum() .. ',' .. Sum(1, 2, 3)
echomsg 'fact=' .. Factorial(5)
"
        ),
        vec![
            "add=5 greet=hello world".to_string(),
            "sum=0,6".to_string(),
            "fact=120".to_string(),
        ]
    );
}

#[test]
fn closures_and_nested_defs() {
    assert_eq!(
        messages(
            "vim9script
def Counter(): number
  var count = 0
  def Bump(): number
    count += 1
    return count
  enddef
  Bump()
  Bump()
  return Bump()
enddef
echomsg 'counter=' .. Counter()
"
        ),
        vec!["counter=3".to_string()]
    );
}

#[test]
fn lambdas_and_higher_order_builtins() {
    assert_eq!(
        messages(
            "vim9script
const Inc = (n) => n + 1
echomsg 'inc=' .. Inc(2)
echomsg 'mapped=' .. ['a', 'b']->map((_, v) => v->toupper())->join('')
echomsg 'filtered=' .. string([8, -2, 9]->filter((_, v) => v > 0))
echomsg 'reduced=' .. [1, 2, 3]->reduce((acc, v) => acc + v)
const Block = (n: number): list<number> => {
  var result: list<number> = []
  for i in range(n)
    result->add(i)
  endfor
  return result
}
echomsg 'block=' .. string(Block(3))
"
        ),
        vec![
            "inc=3".to_string(),
            "mapped=AB".to_string(),
            "filtered=[8, 9]".to_string(),
            "reduced=6".to_string(),
            "block=[0, 1, 2]".to_string(),
        ]
    );
}

#[test]
fn destructuring_and_string_iteration() {
    assert_eq!(
        messages(
            "vim9script
var pair = [10, 20]
var [first, second] = pair
var [head, middle; rest] = [1, 2, 3, 4]
var chars = ''
for ch in 'abc'
  chars ..= ch
endfor
echomsg first .. ',' .. second .. ' ' .. head .. ',' .. middle .. ',' .. string(rest)
echomsg chars
"
        ),
        vec!["10,20 1,2,[3, 4]".to_string(), "abc".to_string(),]
    );
}

#[test]
fn try_catch_finally_and_v_exception() {
    assert_eq!(
        messages(
            "vim9script
var message = ''
try
  throw 'boom'
catch /boom/
  message = 'caught: ' .. v:exception
catch
  message = 'other'
finally
  echomsg 'finally'
endtry
echomsg message
"
        ),
        vec!["finally".to_string(), "caught: boom".to_string()]
    );
}

#[test]
fn heredoc_trim_and_eval() {
    assert_eq!(
        messages(
            "vim9script
var plain =<< END
raw one
raw two
END
var trimmed =<< trim END2
  alpha
  beta
END2
var evaluated =<< trim eval END3
  value is {1 + 2}
END3
echomsg plain->join('|')
echomsg trimmed->join('|')
echomsg evaluated[0]
"
        ),
        vec![
            "raw one|raw two".to_string(),
            "alpha|beta".to_string(),
            "value is 3".to_string(),
        ]
    );
}

#[test]
fn generic_function_calls() {
    assert_eq!(
        messages(
            "vim9script
def First<T>(values: list<T>): T
  return values[0]
enddef
def Identity<T>(value: T): T
  return value
enddef
echomsg 'first=' .. First<number>([10, 20])
echomsg 'id=' .. Identity<string>('x')
"
        ),
        vec!["first=10".to_string(), "id=x".to_string()]
    );
}

#[test]
fn ex_commands_and_options() {
    assert_eq!(
        messages(
            "vim9script
execute 'echomsg \"from-execute\"'
set tabstop=4
set shiftwidth=4
echomsg 'ts=' .. &tabstop .. ' sw=' .. &shiftwidth
normal! ihello
normal! A world
echomsg 'line=' .. getline(1)
def InFunction(): string
  set number
  normal! osecond
  return getline(2) .. ' number=' .. &number
enddef
echomsg 'in=' .. InFunction()
"
        ),
        vec![
            "from-execute".to_string(),
            "ts=4 sw=4".to_string(),
            "line=hello world".to_string(),
            "in=second number=true".to_string(),
        ]
    );
}

#[test]
fn const_and_final_are_locked() {
    let error = run_error(
        "vim9script
const C = 1
C = 2
",
    );
    assert!(
        error.contains("E1018") || error.contains("read-only"),
        "{error}"
    );
}

#[test]
fn classes_static_members_and_methods() {
    assert_eq!(
        messages(
            "vim9script
class Counter
  public var count: number = 0
  var step: number = 1
  static var instances: number = 0
  def new(step: number = 1)
    this.step = step
    Counter.instances += 1
  enddef
  def Incr()
    this.count += this.step
  enddef
endclass
var counter = Counter.new(2)
counter.Incr()
counter.Incr()
echomsg 'count=' .. counter.count .. ' instances=' .. Counter.instances
"
        ),
        vec!["count=4 instances=1".to_string()]
    );
}

#[test]
fn abstract_classes_and_enums() {
    assert_eq!(
        messages(
            "vim9script
abstract class Shape
  public var color: string = 'black'
  abstract def Area(): number
  def Describe(): string
    return this.color .. ':' .. this.Area()
  enddef
endclass
class Square extends Shape
  public var side: number
  def new(side: number)
    this.side = side
  enddef
  def Area(): number
    return this.side * this.side
  enddef
endclass
enum Planet
  Earth(1, false),
  Mars(2, false)
  var moons: number
  var rings: bool
  def new(this.moons, this.rings)
  enddef
endenum
echomsg Square.new(3).Describe()
echomsg 'earth=' .. Planet.Earth.moons .. ',' .. Planet.Earth.rings
echomsg 'name=' .. Planet.Mars.name .. ' ordinal=' .. Planet.Mars.ordinal
"
        ),
        vec![
            "black:9".to_string(),
            "earth=1,false".to_string(),
            "name=Mars ordinal=1".to_string(),
        ]
    );
}

#[test]
fn imports_resolve_relative_to_the_script() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rvim-vim9-import-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("helper.vim"),
        "vim9script
export def Greet(name: string): string
  return 'hi ' .. name
enddef
export const ANSWER = 42
",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.vim"),
        "vim9script
import './helper.vim' as helper
echomsg helper.Greet('vim')
echomsg 'answer=' .. helper.ANSWER
",
    )
    .unwrap();
    let output = super::eval::run_script_file(&dir.join("main.vim")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        output.messages,
        vec!["hi vim".to_string(), "answer=42".to_string()]
    );
}
