//! Conditional fields and formulas. A field may show only when other fields hold certain values,
//! and a `formula` field computes a number from others, such as `{Hours} * {Rate} * 1.25`.
//! Fields refer to each other by label. The signing page runs the same rules
//! (`web/src/lib/field-logic.ts`); both run the cases in `web/src/lib/field-logic.cases.json`.
//! Expressions are parsed by hand into a small tree: there is no `eval` and no way to call
//! anything but the functions listed in `call`.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::FieldKind;
use super::values::{self, NumberFormat};
use crate::error::{Result, bad_request};

pub const MAX_CONDITIONS: usize = 10;
pub const MAX_FORMULA_CHARS: usize = 500;
const MAX_CONDITION_VALUE_CHARS: usize = 200;
const MAX_DEPTH: usize = 32;
const MAX_DECIMALS: usize = 10;
const MAX_DIGITS: usize = 15;
const MAX_OPERATORS: usize = 64;
const MAX_CHAIN: usize = 32;
const MAX_NESTED_OUTCOMES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    Equals,
    NotEquals,
    /// Has a value; for a checkbox, is ticked.
    Filled,
    Empty,
    /// A multi-select list has the option ticked, or a text contains the value.
    Contains,
    Greater,
    Less,
}

impl ConditionOp {
    pub fn takes_value(self) -> bool {
        !matches!(self, ConditionOp::Filled | ConditionOp::Empty)
    }
}

/// One rule of when a field is shown. A field with several rules shows only when all hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    /// The label of the field the rule looks at.
    pub field: String,
    pub op: ConditionOp,
    /// What to compare with: an option, a number, a date as YYYY-MM-DD or text. Not used by `filled` and `empty`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// What the rules need to know about a field.
#[derive(Debug, Clone)]
pub struct Item<'a> {
    pub label: &'a str,
    pub kind: FieldKind,
    pub value: Option<&'a str>,
    pub conditions: &'a [Condition],
    pub formula: Option<&'a str>,
    pub format: Option<&'a str>,
}

/// Whether a field is shown, and its value: cleared when hidden, computed for a formula.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub visible: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Val {
    Empty,
    Num(f64),
    Text(String),
    Bool(bool),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Num(f64),
    Text(String),
    Bool(bool),
    Ref(String),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Cmp(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

const FUNCTIONS: &[(&str, usize, usize)] = &[
    ("round", 1, 2),
    ("floor", 1, 1),
    ("ceil", 1, 1),
    ("abs", 1, 1),
    ("min", 1, 50),
    ("max", 1, 50),
    ("sum", 1, 50),
    ("if", 3, 3),
    ("and", 1, 50),
    ("or", 1, 50),
    ("not", 1, 1),
    ("contains", 2, 2),
];

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Text(String),
    Ref(String),
    Word(String),
    Op(&'static str),
}

fn tokens(source: &str) -> std::result::Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            _ if c.is_whitespace() => i += 1,
            '{' => {
                let end = chars[i + 1..].iter().position(|c| *c == '}').ok_or("A field name has no closing brace")?;
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if name.trim().is_empty() {
                    return Err("A field name between braces is empty".into());
                }
                out.push(Token::Ref(name.trim().to_string()));
                i += end + 2;
            }
            '"' => {
                let end = chars[i + 1..].iter().position(|c| *c == '"').ok_or("A text has no closing quote")?;
                out.push(Token::Text(chars[i + 1..i + 1 + end].iter().collect()));
                i += end + 2;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n = values::number_value(&text).filter(|_| !text.starts_with('.') && !text.ends_with('.'));
                out.push(Token::Num(n.ok_or_else(|| format!("{text} is not a number"))?));
            }
            _ if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                out.push(Token::Word(chars[start..i].iter().collect::<String>().to_lowercase()));
            }
            _ => {
                let next = chars.get(i + 1).copied();
                let (op, len): (&'static str, usize) = match (c, next) {
                    ('!', Some('=')) => ("!=", 2),
                    ('<', Some('>')) => ("!=", 2),
                    ('<', Some('=')) => ("<=", 2),
                    ('>', Some('=')) => (">=", 2),
                    ('=', Some('=')) => ("=", 2),
                    ('=', _) => ("=", 1),
                    ('<', _) => ("<", 1),
                    ('>', _) => (">", 1),
                    ('+', _) => ("+", 1),
                    ('-' | '\u{2212}', _) => ("-", 1),
                    ('*' | '\u{d7}', _) => ("*", 1),
                    ('/' | '\u{f7}', _) => ("/", 1),
                    ('(', _) => ("(", 1),
                    (')', _) => (")", 1),
                    (',' | ';', _) => (",", 1),
                    _ => return Err(format!("\u{201c}{c}\u{201d} cannot be used in a formula")),
                };
                out.push(Token::Op(op));
                i += len;
            }
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Token::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn deeper(&mut self) -> std::result::Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err("The formula is nested too deeply".into()) } else { Ok(()) }
    }

    fn expr(&mut self) -> std::result::Result<Expr, String> {
        self.deeper()?;
        let left = self.sum()?;
        let op = match self.peek() {
            Some(Token::Op(op @ ("=" | "!=" | "<" | "<=" | ">" | ">="))) => *op,
            _ => {
                self.depth -= 1;
                return Ok(left);
            }
        };
        self.at += 1;
        let right = self.sum()?;
        self.depth -= 1;
        Ok(Expr::Cmp(op, Box::new(left), Box::new(right)))
    }

    fn sum(&mut self) -> std::result::Result<Expr, String> {
        let mut left = self.product()?;
        loop {
            let op = if self.eat("+") {
                '+'
            } else if self.eat("-") {
                '-'
            } else {
                return Ok(left);
            };
            left = Expr::Bin(op, Box::new(left), Box::new(self.product()?));
        }
    }

    fn product(&mut self) -> std::result::Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat("*") {
                '*'
            } else if self.eat("/") {
                '/'
            } else {
                return Ok(left);
            };
            left = Expr::Bin(op, Box::new(left), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> std::result::Result<Expr, String> {
        if self.eat("-") {
            self.deeper()?;
            let inner = self.unary()?;
            self.depth -= 1;
            return Ok(Expr::Neg(Box::new(inner)));
        }
        if self.eat("+") {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> std::result::Result<Expr, String> {
        let token = self.peek().cloned().ok_or("The formula ends too early")?;
        self.at += 1;
        match token {
            Token::Num(n) => Ok(Expr::Num(n)),
            Token::Text(s) => Ok(Expr::Text(s)),
            Token::Ref(name) => Ok(Expr::Ref(name)),
            Token::Word(w) if w == "true" || w == "false" => Ok(Expr::Bool(w == "true")),
            Token::Word(w) => {
                let (_, min, max) =
                    *FUNCTIONS.iter().find(|(name, _, _)| *name == w).ok_or_else(|| format!("There is no function called {w}"))?;
                if !self.eat("(") {
                    return Err(format!("Write {w}( ) with its values in brackets"));
                }
                let mut args = Vec::new();
                if !self.eat(")") {
                    loop {
                        args.push(self.expr()?);
                        if self.eat(")") {
                            break;
                        }
                        if !self.eat(",") {
                            return Err(format!("Separate the values of {w}( ) with commas"));
                        }
                    }
                }
                if args.len() < min || args.len() > max {
                    return Err(match (min, max) {
                        (a, b) if a == b => format!("{w}( ) takes {a} values"),
                        (a, b) => format!("{w}( ) takes from {a} to {b} values"),
                    });
                }
                Ok(Expr::Call(w, args))
            }
            Token::Op("(") => {
                let inner = self.expr()?;
                if !self.eat(")") {
                    return Err("A bracket is not closed".into());
                }
                Ok(inner)
            }
            Token::Op(op) => Err(format!("\u{201c}{op}\u{201d} is in an unexpected place")),
        }
    }
}

fn parse(source: &str) -> std::result::Result<Expr, String> {
    if source.chars().count() > MAX_FORMULA_CHARS {
        return Err(format!("A formula is at most {MAX_FORMULA_CHARS} characters"));
    }
    let mut parser = Parser { tokens: tokens(source)?, at: 0, depth: 0 };
    if parser.tokens.is_empty() {
        return Err("The formula is empty".into());
    }
    let expr = parser.expr()?;
    if parser.at < parser.tokens.len() {
        return Err("Something follows the end of the formula; is an operator missing?".into());
    }
    Ok(expr)
}

fn refs(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Ref(name) if !out.contains(name) => out.push(name.clone()),
        Expr::Neg(e) => refs(e, out),
        Expr::Bin(_, a, b) | Expr::Cmp(_, a, b) => {
            refs(a, out);
            refs(b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| refs(a, out)),
        _ => {}
    }
}

fn operators(expr: &Expr) -> usize {
    match expr {
        Expr::Neg(e) => 1 + operators(e),
        Expr::Bin(_, a, b) | Expr::Cmp(_, a, b) => 1 + operators(a) + operators(b),
        Expr::Call(_, args) => 1 + args.iter().map(operators).sum::<usize>(),
        _ => 0,
    }
}

/// The labels a formula uses, or why it cannot be read.
pub fn formula_refs(source: &str) -> std::result::Result<Vec<String>, String> {
    let mut out = Vec::new();
    refs(&parse(source)?, &mut out);
    Ok(out)
}

/// The labels a field's conditions and formula use.
pub fn dependencies(item: &Item) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in item.conditions {
        let name = c.field.trim().to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    if let Some(Ok(names)) = item.formula.map(formula_refs) {
        for name in names {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// A finite number written out in full, as the shortest decimal that reads back as the same number.
fn plain(n: f64) -> String {
    let text = format!("{n}");
    if text == "-0" { "0".into() } else { text }
}

fn round_to(n: f64, decimals: usize) -> Option<f64> {
    let text = plain(n);
    let negative = text.starts_with('-');
    let (int, frac) = text.trim_start_matches('-').split_once('.').unwrap_or((text.trim_start_matches('-'), ""));
    let (int, frac) = values::round_half_up(int, frac, decimals);
    let out: f64 = format!("{}{int}.{frac}0", if negative { "-" } else { "" }).parse().ok()?;
    Some(if out == 0.0 { 0.0 } else { out })
}

/// A computed number in the stored form of a number field: rounded to the format's decimals, or
/// to at most ten, within fifteen digits.
fn canonical(n: f64, format: Option<&str>) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    let text = plain(n);
    let negative = text.starts_with('-');
    let (int, frac) = text.trim_start_matches('-').split_once('.').unwrap_or((text.trim_start_matches('-'), ""));
    let int = int.trim_start_matches('0');
    if int.len() > MAX_DIGITS {
        return None;
    }
    let wanted = format.and_then(NumberFormat::parse).map_or(MAX_DECIMALS, |f| f.decimals());
    let (int, frac) = values::round_half_up(int, frac, wanted.min(MAX_DIGITS - int.len()));
    if int.trim_start_matches('0').len() > MAX_DIGITS {
        return None;
    }
    let int = int.trim_start_matches('0');
    let frac = frac.trim_end_matches('0');
    let mut out = String::new();
    if negative && (!int.is_empty() || !frac.is_empty()) {
        out.push('-');
    }
    out.push_str(if int.is_empty() { "0" } else { int });
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    Some(out)
}

fn number_of(v: &Val) -> Option<f64> {
    match v {
        Val::Num(n) => Some(*n),
        Val::Text(s) => values::number_value(s.trim()),
        _ => None,
    }
}

fn arithmetic(v: &Val) -> Option<f64> {
    match v {
        Val::Empty => Some(0.0),
        Val::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Val::List(_) => None,
        _ => number_of(v),
    }
}

fn text_of(v: &Val) -> String {
    match v {
        Val::Empty => String::new(),
        Val::Num(n) => plain(*n),
        Val::Text(s) => s.clone(),
        Val::Bool(b) => b.to_string(),
        Val::List(l) => l.join(", "),
    }
}

fn filled(v: &Val) -> bool {
    match v {
        Val::Empty => false,
        Val::Num(_) => true,
        Val::Text(s) => !s.is_empty(),
        Val::Bool(b) => *b,
        Val::List(l) => !l.is_empty(),
    }
}

fn truthy(v: &Val) -> bool {
    match v {
        Val::Num(n) => *n != 0.0,
        _ => filled(v),
    }
}

fn equal(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Empty, Val::Empty) => true,
        (Val::Num(_), _) | (_, Val::Num(_)) => matches!((number_of(a), number_of(b)), (Some(x), Some(y)) if x == y),
        _ => text_of(a).to_lowercase() == text_of(b).to_lowercase(),
    }
}

fn order(a: &Val, b: &Val) -> Option<std::cmp::Ordering> {
    match (number_of(a), number_of(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y),
        _ => match (a, b) {
            (Val::Text(x), Val::Text(y)) if !x.is_empty() && !y.is_empty() => Some(x.cmp(y)),
            _ => None,
        },
    }
}

fn contains(a: &Val, b: &Val) -> bool {
    match a {
        Val::List(items) => items.iter().any(|i| equal(&Val::Text(i.clone()), b)),
        Val::Text(s) => {
            let needle = text_of(b).to_lowercase();
            !needle.is_empty() && s.to_lowercase().contains(&needle)
        }
        _ => false,
    }
}

type Lookup<'a> = dyn FnMut(&str) -> Val + 'a;

fn eval(expr: &Expr, lookup: &mut Lookup) -> Option<Val> {
    Some(match expr {
        Expr::Num(n) => Val::Num(*n),
        Expr::Text(s) => Val::Text(s.clone()),
        Expr::Bool(b) => Val::Bool(*b),
        Expr::Ref(name) => lookup(name),
        Expr::Neg(e) => Val::Num(-arithmetic(&eval(e, lookup)?)?),
        Expr::Bin(op, a, b) => {
            let (x, y) = (arithmetic(&eval(a, lookup)?)?, arithmetic(&eval(b, lookup)?)?);
            let n = match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                _ if y == 0.0 => return None,
                _ => x / y,
            };
            if !n.is_finite() {
                return None;
            }
            Val::Num(n)
        }
        Expr::Cmp(op, a, b) => {
            let (x, y) = (eval(a, lookup)?, eval(b, lookup)?);
            let ord = order(&x, &y);
            Val::Bool(match *op {
                "=" => equal(&x, &y),
                "!=" => !equal(&x, &y),
                "<" => ord == Some(std::cmp::Ordering::Less),
                ">" => ord == Some(std::cmp::Ordering::Greater),
                "<=" => matches!(ord, Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)),
                _ => matches!(ord, Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)),
            })
        }
        Expr::Call(name, args) => call(name, args, lookup)?,
    })
}

fn call(name: &str, args: &[Expr], lookup: &mut Lookup) -> Option<Val> {
    if name == "if" {
        let condition = eval(&args[0], lookup)?;
        return eval(&args[if truthy(&condition) { 1 } else { 2 }], lookup);
    }
    let mut vals = Vec::with_capacity(args.len());
    for a in args {
        vals.push(eval(a, lookup)?);
    }
    let num = |v: &Val| arithmetic(v);
    Some(match name {
        "round" => {
            let decimals = match vals.get(1) {
                Some(d) => num(d)?,
                None => 0.0,
            };
            if !(0.0..=6.0).contains(&decimals) || decimals.fract() != 0.0 {
                return None;
            }
            Val::Num(round_to(num(&vals[0])?, decimals as usize)?)
        }
        "floor" => Val::Num(num(&vals[0])?.floor()),
        "ceil" => Val::Num(num(&vals[0])?.ceil()),
        "abs" => Val::Num(num(&vals[0])?.abs()),
        "min" | "max" | "sum" => {
            let mut ns = Vec::with_capacity(vals.len());
            for v in &vals {
                ns.push(num(v)?);
            }
            let first = ns[0];
            Val::Num(match name {
                "min" => ns.iter().copied().fold(first, f64::min),
                "max" => ns.iter().copied().fold(first, f64::max),
                _ => ns.iter().sum(),
            })
        }
        "and" => Val::Bool(vals.iter().all(truthy)),
        "or" => Val::Bool(vals.iter().any(truthy)),
        "not" => Val::Bool(!truthy(&vals[0])),
        "contains" => Val::Bool(contains(&vals[0], &vals[1])),
        _ => return None,
    })
}

fn holds(condition: &Condition, value: &Val) -> bool {
    let other = Val::Text(condition.value.clone().unwrap_or_default());
    match condition.op {
        ConditionOp::Equals => equal(value, &other),
        ConditionOp::NotEquals => !equal(value, &other),
        ConditionOp::Filled => filled(value),
        ConditionOp::Empty => !filled(value),
        ConditionOp::Contains => contains(value, &other),
        ConditionOp::Greater => order(value, &other) == Some(std::cmp::Ordering::Greater),
        ConditionOp::Less => order(value, &other) == Some(std::cmp::Ordering::Less),
    }
}

fn value_of(kind: FieldKind, value: Option<&str>) -> Val {
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    match kind {
        FieldKind::Checkbox => Val::Bool(value == Some("true")),
        FieldKind::Number | FieldKind::Formula => value.and_then(values::number_value).map_or(Val::Empty, Val::Num),
        FieldKind::Multiselect => match value {
            Some(v) => Val::List(serde_json::from_str(v).unwrap_or_default()),
            None => Val::Empty,
        },
        _ => value.map_or(Val::Empty, |v| Val::Text(v.to_string())),
    }
}

struct Run<'a, 'b> {
    items: &'b [Item<'a>],
    by_label: HashMap<&'a str, Vec<usize>>,
    done: Vec<Option<Outcome>>,
    busy: Vec<bool>,
    nested: usize,
}

impl<'a> Run<'a, '_> {
    fn label_value(&mut self, label: &str) -> Val {
        let Some(indexes) = self.by_label.get(label.trim()).cloned() else { return Val::Empty };
        let mut chosen = Val::Empty;
        for i in indexes {
            let outcome = self.outcome(i);
            let item = &self.items[i];
            if item.kind == FieldKind::Radio {
                if let Some(v) = outcome.value.filter(|_| outcome.visible) {
                    chosen = Val::Text(v);
                }
            } else {
                return if outcome.visible { value_of(item.kind, outcome.value.as_deref()) } else { Val::Empty };
            }
        }
        chosen
    }

    fn outcome(&mut self, i: usize) -> Outcome {
        if let Some(done) = &self.done[i] {
            return done.clone();
        }
        // A loop is refused when fields are saved; if one slips through, the field stays hidden.
        if self.busy[i] || self.nested >= MAX_NESTED_OUTCOMES {
            return Outcome { visible: false, value: None };
        }
        self.busy[i] = true;
        self.nested += 1;
        let item = self.items[i].clone();
        let mut visible = true;
        for c in item.conditions {
            let value = self.label_value(&c.field);
            if !holds(c, &value) {
                visible = false;
                break;
            }
        }
        let value = if !visible {
            None
        } else if item.kind == FieldKind::Formula {
            item.formula.and_then(|f| parse(f).ok()).and_then(|expr| {
                let result = eval(&expr, &mut |name: &str| self.label_value(name))?;
                canonical(arithmetic(&result)?, item.format)
            })
        } else {
            item.value.map(str::to_string)
        };
        self.busy[i] = false;
        self.nested -= 1;
        let outcome = Outcome { visible, value };
        self.done[i] = Some(outcome.clone());
        outcome
    }
}

/// Works out which fields are shown and what each formula comes to.
pub fn evaluate(items: &[Item]) -> Vec<Outcome> {
    let mut by_label: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let label = item.label.trim();
        if !label.is_empty() {
            by_label.entry(label).or_default().push(i);
        }
    }
    let deps: Vec<Vec<usize>> = items
        .iter()
        .map(|item| dependencies(item).iter().flat_map(|label| by_label.get(label.as_str()).into_iter().flatten().copied()).collect())
        .collect();
    let mut run = Run { items, by_label, done: vec![None; items.len()], busy: vec![false; items.len()], nested: 0 };
    // Dependencies first, with a stack of our own: stored fields may chain further than the call
    // stack reaches, and a stack overflow would take the whole process down.
    let mut seen = vec![false; items.len()];
    for start in 0..items.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((i, next)) = stack.last_mut() {
            let i = *i;
            if let Some(&d) = deps[i].get(*next) {
                *next += 1;
                if !seen[d] {
                    seen[d] = true;
                    stack.push((d, 0));
                }
            } else {
                run.outcome(i);
                stack.pop();
            }
        }
    }
    (0..items.len()).map(|i| run.outcome(i)).collect()
}

/// Evaluates stored fields, with `values` in place of their stored values when given.
pub fn evaluate_rows(fields: &[super::FieldRow], values: Option<&[Option<String>]>) -> Vec<Outcome> {
    let conditions: Vec<Vec<Condition>> = fields.iter().map(super::FieldRow::conditions).collect();
    let items: Vec<Item> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| Item {
            label: &f.label,
            kind: FieldKind::parse(&f.kind).unwrap_or(FieldKind::Text),
            value: values.map_or(f.value.as_deref(), |v| v[i].as_deref()),
            conditions: &conditions[i],
            formula: f.formula.as_deref(),
            format: f.format.as_deref(),
        })
        .collect();
    evaluate(&items)
}

/// Which fields the rules of the `wanted` ones look at, directly or through other fields.
pub fn needed(fields: &[super::FieldRow], wanted: &[bool]) -> Vec<bool> {
    let conditions: Vec<Vec<Condition>> = fields.iter().map(super::FieldRow::conditions).collect();
    let mut out = wanted.to_vec();
    let mut queue: Vec<usize> = (0..fields.len()).filter(|i| wanted[*i]).collect();
    while let Some(i) = queue.pop() {
        let f = &fields[i];
        let item = Item {
            label: &f.label,
            kind: FieldKind::Text,
            value: None,
            conditions: &conditions[i],
            formula: f.formula.as_deref(),
            format: None,
        };
        for label in dependencies(&item) {
            for (j, other) in fields.iter().enumerate() {
                if !out[j] && other.label.trim() == label {
                    out[j] = true;
                    queue.push(j);
                }
            }
        }
    }
    out
}

/// Who fills in a field, for checking what its rules may look at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Owner {
    Sender,
    /// A party, with their place in the signing order when the document is signed in order.
    Party(uuid::Uuid, Option<i32>),
}

/// Checks the conditions and formulas of a document's fields when they are saved: every label they
/// use names one field, a party only looks at fields that are filled in before they sign, and
/// no field depends on itself.
pub fn check(items: &[Item], owners: &[Owner]) -> Result<()> {
    let mut by_label: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let label = item.label.trim();
        if !label.is_empty() {
            by_label.entry(label).or_default().push(i);
        }
    }
    let mut deps: Vec<Vec<usize>> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let name = if item.label.trim().is_empty() { item.kind.as_str() } else { item.label.trim() };
        if item.conditions.len() > MAX_CONDITIONS {
            return Err(bad_request(format!("A field can have at most {MAX_CONDITIONS} conditions")));
        }
        for c in item.conditions {
            let value = c.value.as_deref().map(str::trim).unwrap_or_default();
            if c.op.takes_value() && value.is_empty() {
                return Err(bad_request(format!("A condition of \u{201c}{name}\u{201d} needs a value to compare with")));
            }
            if value.chars().count() > MAX_CONDITION_VALUE_CHARS || value.chars().any(char::is_control) {
                return Err(bad_request(format!("Keep the values of conditions under {MAX_CONDITION_VALUE_CHARS} characters")));
            }
        }
        match (item.kind, item.formula) {
            (FieldKind::Formula, Some(f)) => {
                let expr = parse(f).map_err(|e| bad_request(format!("The formula of \u{201c}{name}\u{201d} cannot be read: {e}")))?;
                if operators(&expr) > MAX_OPERATORS {
                    return Err(bad_request(format!(
                        "The formula of \u{201c}{name}\u{201d} is too long: use at most {MAX_OPERATORS} operators"
                    )));
                }
            }
            (FieldKind::Formula, None) => return Err(bad_request(format!("Write the formula of \u{201c}{name}\u{201d}"))),
            (_, Some(_)) => return Err(bad_request("Only formula fields have a formula")),
            _ => {}
        }
        let mut mine = Vec::new();
        for label in dependencies(item) {
            let targets = by_label.get(label.as_str()).ok_or_else(|| {
                bad_request(format!("\u{201c}{name}\u{201d} refers to \u{201c}{label}\u{201d}, but no field has that label"))
            })?;
            let one = targets.len() == 1 || targets.iter().all(|t| items[*t].kind == FieldKind::Radio && owners[*t] == owners[targets[0]]);
            if !one {
                return Err(bad_request(format!(
                    "Several fields are labelled \u{201c}{label}\u{201d}; give them different labels so \u{201c}{name}\u{201d} knows which to use"
                )));
            }
            if targets.contains(&i) {
                return Err(bad_request(format!("\u{201c}{name}\u{201d} cannot depend on itself")));
            }
            let allowed = match (owners[i], owners[targets[0]]) {
                (_, Owner::Sender) => true,
                (Owner::Sender, Owner::Party(..)) => item.kind != FieldKind::Formula,
                (Owner::Party(a, _), Owner::Party(b, _)) if a == b => true,
                (Owner::Party(_, Some(mine)), Owner::Party(_, Some(theirs))) => theirs < mine,
                _ => false,
            };
            if !allowed {
                return Err(bad_request(if owners[i] == Owner::Sender {
                    format!("A formula you fill in can only use your own fields, not \u{201c}{label}\u{201d}")
                } else {
                    format!(
                        "\u{201c}{name}\u{201d} can only use fields of the same party, your own, or of parties who sign earlier in order, not \u{201c}{label}\u{201d}"
                    )
                }));
            }
            mine.extend(targets.iter().copied());
        }
        deps.push(mine);
    }
    // 0 = not visited, 1 = on the current path, 2 = done. `chain` is the longest run of fields below each one.
    let mut state = vec![0u8; items.len()];
    let mut chain = vec![0usize; items.len()];
    for start in 0..items.len() {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut stack = vec![(start, 0usize)];
        while let Some((i, next)) = stack.last_mut() {
            let i = *i;
            if let Some(&d) = deps[i].get(*next) {
                *next += 1;
                match state[d] {
                    0 => {
                        state[d] = 1;
                        stack.push((d, 0));
                    }
                    1 => {
                        return Err(bad_request(format!(
                            "\u{201c}{}\u{201d} depends on itself through other fields' conditions or formulas",
                            items[i].label.trim()
                        )));
                    }
                    _ => {}
                }
            } else {
                chain[i] = deps[i].iter().map(|d| chain[*d] + 1).max().unwrap_or(0);
                if chain[i] > MAX_CHAIN {
                    return Err(bad_request(format!(
                        "\u{201c}{}\u{201d} builds on too many other fields in a row; keep it to at most {MAX_CHAIN}",
                        items[i].label.trim()
                    )));
                }
                state[i] = 2;
                stack.pop();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CaseField {
        label: String,
        kind: String,
        #[serde(default)]
        value: Option<String>,
        #[serde(default)]
        conditions: Vec<Condition>,
        #[serde(default)]
        formula: Option<String>,
        #[serde(default)]
        format: Option<String>,
    }

    #[derive(Deserialize)]
    struct Document {
        name: String,
        fields: Vec<CaseField>,
        visible: Vec<bool>,
        values: Vec<Option<String>>,
    }

    #[derive(Deserialize)]
    struct Invalid {
        formula: String,
    }

    #[derive(Deserialize)]
    struct Cases {
        documents: Vec<Document>,
        invalid: Vec<Invalid>,
    }

    fn cases() -> Cases {
        serde_json::from_str(include_str!("../../../../web/src/lib/field-logic.cases.json")).unwrap()
    }

    fn items(fields: &[CaseField]) -> Vec<Item<'_>> {
        fields
            .iter()
            .map(|f| Item {
                label: &f.label,
                kind: FieldKind::parse(&f.kind).unwrap(),
                value: f.value.as_deref(),
                conditions: &f.conditions,
                formula: f.formula.as_deref(),
                format: f.format.as_deref(),
            })
            .collect()
    }

    #[test]
    fn shared_documents() {
        for doc in cases().documents {
            let out = evaluate(&items(&doc.fields));
            let visible: Vec<bool> = out.iter().map(|o| o.visible).collect();
            let values: Vec<Option<String>> = out.into_iter().map(|o| o.value).collect();
            assert_eq!(visible, doc.visible, "{}: visible", doc.name);
            assert_eq!(values, doc.values, "{}: values", doc.name);
        }
    }

    #[test]
    fn shared_invalid_formulas() {
        for case in cases().invalid {
            assert!(parse(&case.formula).is_err(), "{:?} should not parse", case.formula);
        }
    }

    #[test]
    fn refuses_loops_and_unknown_labels() {
        let a = [Condition { field: "B".into(), op: ConditionOp::Filled, value: None }];
        let b = [Condition { field: "A".into(), op: ConditionOp::Filled, value: None }];
        let item = |label, conditions| Item { label, kind: FieldKind::Text, value: None, conditions, formula: None, format: None };
        let party = Owner::Party(uuid::Uuid::nil(), None);
        assert!(check(&[item("A", &a), item("B", &b)], &[party, party]).is_err());
        assert!(check(&[item("A", &a), item("B", &[])], &[party, party]).is_ok());
        assert!(check(&[item("A", &a)], &[party]).is_err());
        let other = Owner::Party(uuid::Uuid::max(), None);
        assert!(check(&[item("A", &a), item("B", &[])], &[party, other]).is_err(), "another party who signs at the same time");
        let first = Owner::Party(uuid::Uuid::max(), Some(1));
        let second = Owner::Party(uuid::Uuid::nil(), Some(2));
        assert!(check(&[item("A", &a), item("B", &[])], &[second, first]).is_ok(), "a party who signed earlier");
        assert!(check(&[item("A", &a), item("B", &[])], &[first, second]).is_err(), "a party who signs later");
        let formula = Item { formula: Some("{B} * 2"), kind: FieldKind::Formula, ..item("A", &[]) };
        assert!(check(&[formula.clone(), item("B", &[])], &[Owner::Sender, party]).is_err(), "a sender formula using a party field");
        let deep = format!("{}1{}", "(".repeat(40), ")".repeat(40));
        assert!(parse(&deep).is_err());
    }

    fn chain_of(fields: usize, formula_tail: &str) -> (Vec<String>, Vec<String>) {
        let labels = (0..fields).map(|i| format!("F{i}")).collect();
        let formulas = (0..fields).map(|i| format!("{{F{}}}{formula_tail}", i + 1)).collect();
        (labels, formulas)
    }

    fn chain_items<'a>(labels: &'a [String], formulas: &'a [String]) -> Vec<Item<'a>> {
        let last = labels.len() - 1;
        labels
            .iter()
            .enumerate()
            .map(|(i, label)| Item {
                label,
                kind: if i == last { FieldKind::Number } else { FieldKind::Formula },
                value: Some("1"),
                conditions: &[],
                formula: if i == last { None } else { Some(&formulas[i]) },
                format: None,
            })
            .collect()
    }

    #[test]
    fn long_field_chains_are_refused_when_saved() {
        let (labels, formulas) = chain_of(40, "+1");
        let items = chain_items(&labels, &formulas);
        let owners = vec![Owner::Sender; items.len()];
        assert!(check(&items, &owners).unwrap_err().message.contains("too many other fields"));
        let (labels, formulas) = chain_of(20, "+1");
        let items = chain_items(&labels, &formulas);
        assert!(check(&items, &vec![Owner::Sender; items.len()]).is_ok());
    }

    #[test]
    fn long_formulas_are_refused_when_saved() {
        let (labels, formulas) = chain_of(2, &"+1".repeat(MAX_OPERATORS + 1));
        let items = chain_items(&labels, &formulas);
        assert!(check(&items, &vec![Owner::Sender; items.len()]).unwrap_err().message.contains("too long"));
    }

    #[test]
    fn stored_chains_evaluate_on_a_small_stack() {
        // Fields saved before the limits: as long as a formula could be, chained further than a stack reaches.
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let (labels, formulas) = chain_of(500, &"+1".repeat(235));
                let items = chain_items(&labels, &formulas);
                let out = evaluate(&items);
                assert_eq!(out[0].value.as_deref(), Some((1 + 499 * 235).to_string().as_str()));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn a_stored_loop_of_any_length_leaves_fields_hidden() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let labels: Vec<String> = (0..2000).map(|i| format!("F{i}")).collect();
                let formulas: Vec<String> = (0..2000).map(|i| format!("{{F{}}}+1", (i + 1) % 2000)).collect();
                let items: Vec<Item> = labels
                    .iter()
                    .zip(&formulas)
                    .map(|(label, formula)| Item {
                        label,
                        kind: FieldKind::Formula,
                        value: None,
                        conditions: &[],
                        formula: Some(formula),
                        format: None,
                    })
                    .collect();
                assert_eq!(evaluate(&items).len(), 2000);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
