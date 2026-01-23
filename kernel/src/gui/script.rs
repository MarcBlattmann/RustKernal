//! PursuitScript - Simple scripting language for .pa apps
//!
//! ## Syntax Overview
//!
//! ### Variables
//! ```
//! var count = 0
//! var name = "Hello"
//! var enabled = true
//! ```
//!
//! ### Assignments
//! ```
//! count = count + 1
//! name = "World"
//! ```
//!
//! ### Functions
//! ```
//! func increment() {
//!     count = count + 1
//! }
//!
//! func add(a, b) {
//!     return a + b
//! }
//! ```
//!
//! ### If Statements
//! ```
//! if count > 10 {
//!     count = 0
//! }
//!
//! if enabled {
//!     doSomething()
//! } else {
//!     doOther()
//! }
//! ```
//!
//! ### Built-in Functions
//! - `close()` - Close the current window
//! - `open("app_id")` - Open another app
//! - `minimize()` - Minimize the window
//! - `print("message")` - Debug print
//!
//! ### Operators
//! - Arithmetic: `+`, `-`, `*`, `/`, `%`
//! - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
//! - Logical: `&&`, `||`, `!`

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::boxed::Box;
use crate::drivers::drives::DRIVE_MANAGER;

/// Script value types
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<Value>),
}

impl Value {
    /// Convert to display string
    pub fn to_display(&self) -> String {
        match self {
            Value::Null => String::from("null"),
            Value::Int(i) => format!("{}", i),
            Value::Float(f) => format!("{}", f),
            Value::Bool(b) => if *b { String::from("true") } else { String::from("false") },
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_display()).collect();
                format!("[{}]", items.join(", "))
            }
        }
    }
    
    /// Convert to boolean for conditions
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Bool(b) => *b,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => !arr.is_empty(),
        }
    }
    
    /// Convert to integer
    pub fn to_int(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => if *b { 1 } else { 0 },
            _ => 0,
        }
    }
}

/// Expression types
#[derive(Clone, Debug)]
pub enum Expr {
    /// Literal value
    Literal(Value),
    /// Variable reference
    Var(String),
    /// Binary operation
    BinOp {
        left: Box<Expr>,
        op: BinOperator,
        right: Box<Expr>,
    },
    /// Unary operation
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expr>,
    },
    /// Function call
    Call {
        name: String,
        args: Vec<Expr>,
    },
}

/// Binary operators
#[derive(Clone, Debug, PartialEq)]
pub enum BinOperator {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
}

/// Unary operators
#[derive(Clone, Debug)]
pub enum UnaryOperator {
    Not,
    Neg,
}

/// Statement types
#[derive(Clone, Debug)]
pub enum Stmt {
    /// Variable declaration: var x = expr
    VarDecl { name: String, value: Expr },
    /// Assignment: x = expr
    Assign { name: String, value: Expr },
    /// If statement
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_block: Option<Vec<Stmt>>,
    },
    /// While loop
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    /// Function call as statement
    ExprStmt(Expr),
    /// Return statement
    Return(Option<Expr>),
}

/// User-defined function
#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

/// Script actions that need to be handled by the window system
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptAction {
    None,
    Close,
    Open(String),
    Minimize,
}

/// Script engine state
pub struct ScriptEngine {
    /// Global variables
    pub variables: BTreeMap<String, Value>,
    /// User-defined functions
    pub functions: BTreeMap<String, Function>,
    /// Pending action from script execution
    pub pending_action: ScriptAction,
}

impl ScriptEngine {
    /// Create a new script engine
    pub fn new() -> Self {
        Self {
            variables: BTreeMap::new(),
            functions: BTreeMap::new(),
            pending_action: ScriptAction::None,
        }
    }
    
    /// Parse and execute a script block (for initialization)
    pub fn execute_script(&mut self, source: &str) {
        let mut parser = ScriptParser::new(source);
        match parser.parse_script() {
            Ok((vars, funcs)) => {
                // Execute variable declarations
                for stmt in vars {
                    self.execute_stmt(&stmt);
                }
                // Store functions
                for func in funcs {
                    self.functions.insert(func.name.clone(), func);
                }
            }
            Err(_e) => {
                // Silently ignore parse errors for now
            }
        }
    }
    
    /// Execute a single statement (for on_click handlers)
    pub fn execute_inline(&mut self, source: &str) {
        let mut parser = ScriptParser::new(source);
        match parser.parse_inline() {
            Ok(stmts) => {
                for stmt in stmts {
                    self.execute_stmt(&stmt);
                }
            }
            Err(_e) => {
                // Silently ignore errors
            }
        }
    }
    
    /// Execute a statement
    fn execute_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl { name, value } => {
                let val = self.evaluate(value);
                self.variables.insert(name.clone(), val);
            }
            Stmt::Assign { name, value } => {
                let val = self.evaluate(value);
                self.variables.insert(name.clone(), val);
            }
            Stmt::If { condition, then_block, else_block } => {
                let cond = self.evaluate(condition);
                if cond.is_truthy() {
                    for s in then_block {
                        self.execute_stmt(s);
                    }
                } else if let Some(else_stmts) = else_block {
                    for s in else_stmts {
                        self.execute_stmt(s);
                    }
                }
            }
            Stmt::While { condition, body } => {
                let mut iterations = 0;
                while self.evaluate(condition).is_truthy() && iterations < 10000 {
                    for s in body {
                        self.execute_stmt(s);
                    }
                    iterations += 1;
                }
            }
            Stmt::ExprStmt(expr) => {
                self.evaluate(expr);
            }
            Stmt::Return(_) => {
                // Return is handled in function calls
            }
        }
    }
    
    /// Evaluate an expression
    fn evaluate(&mut self, expr: &Expr) -> Value {
        match expr {
            Expr::Literal(v) => v.clone(),
            Expr::Var(name) => {
                self.variables.get(name).cloned().unwrap_or(Value::Null)
            }
            Expr::BinOp { left, op, right } => {
                let l = self.evaluate(left);
                let r = self.evaluate(right);
                self.apply_binop(&l, op, &r)
            }
            Expr::UnaryOp { op, expr } => {
                let v = self.evaluate(expr);
                match op {
                    UnaryOperator::Not => Value::Bool(!v.is_truthy()),
                    UnaryOperator::Neg => Value::Int(-v.to_int()),
                }
            }
            Expr::Call { name, args } => {
                self.call_function(name, args)
            }
        }
    }
    
    /// Apply binary operator
    fn apply_binop(&self, left: &Value, op: &BinOperator, right: &Value) -> Value {
        match op {
            BinOperator::Add => {
                match (left, right) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                    (Value::String(a), Value::String(b)) => {
                        let mut s = a.clone();
                        s.push_str(b);
                        Value::String(s)
                    }
                    (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
                    (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
                    _ => Value::Int(left.to_int() + right.to_int()),
                }
            }
            BinOperator::Sub => Value::Int(left.to_int() - right.to_int()),
            BinOperator::Mul => Value::Int(left.to_int() * right.to_int()),
            BinOperator::Div => {
                let r = right.to_int();
                if r == 0 { Value::Int(0) } else { Value::Int(left.to_int() / r) }
            }
            BinOperator::Mod => {
                let r = right.to_int();
                if r == 0 { Value::Int(0) } else { Value::Int(left.to_int() % r) }
            }
            BinOperator::Eq => Value::Bool(left == right),
            BinOperator::Ne => Value::Bool(left != right),
            BinOperator::Lt => Value::Bool(left.to_int() < right.to_int()),
            BinOperator::Gt => Value::Bool(left.to_int() > right.to_int()),
            BinOperator::Le => Value::Bool(left.to_int() <= right.to_int()),
            BinOperator::Ge => Value::Bool(left.to_int() >= right.to_int()),
            BinOperator::And => Value::Bool(left.is_truthy() && right.is_truthy()),
            BinOperator::Or => Value::Bool(left.is_truthy() || right.is_truthy()),
        }
    }
    
    /// Call a function
    fn call_function(&mut self, name: &str, args: &[Expr]) -> Value {
        // Built-in functions
        match name {
            "close" => {
                self.pending_action = ScriptAction::Close;
                Value::Null
            }
            "open" => {
                if let Some(arg) = args.first() {
                    if let Value::String(app_id) = self.evaluate(arg) {
                        self.pending_action = ScriptAction::Open(app_id);
                    }
                }
                Value::Null
            }
            "minimize" => {
                self.pending_action = ScriptAction::Minimize;
                Value::Null
            }
            "print" => {
                // Debug print - could be implemented later
                Value::Null
            }
            "abs" => {
                if let Some(arg) = args.first() {
                    let v = self.evaluate(arg).to_int();
                    Value::Int(v.abs())
                } else {
                    Value::Int(0)
                }
            }
            "min" => {
                if args.len() >= 2 {
                    let a = self.evaluate(&args[0]).to_int();
                    let b = self.evaluate(&args[1]).to_int();
                    Value::Int(a.min(b))
                } else {
                    Value::Int(0)
                }
            }
            "max" => {
                if args.len() >= 2 {
                    let a = self.evaluate(&args[0]).to_int();
                    let b = self.evaluate(&args[1]).to_int();
                    Value::Int(a.max(b))
                } else {
                    Value::Int(0)
                }
            }
            "listDrives" => {
                let manager = DRIVE_MANAGER.lock();
                let drives = manager.list_drives();
                let arr: Vec<Value> = drives.iter()
                    .map(|(name, _)| Value::String(name.clone()))
                    .collect();
                Value::Array(arr)
            }
            "listFiles" => {
                if let Some(arg) = args.first() {
                    if let Value::String(drive_name) = self.evaluate(arg) {
                        let manager = DRIVE_MANAGER.lock();
                        if let Some(drive) = manager.get_drive(&drive_name) {
                            let files = drive.list_files();
                            let arr: Vec<Value> = files.iter()
                                .map(|(name, is_dir)| {
                                    // Return as "name|isDir" string for simplicity
                                    let suffix = if *is_dir { "/" } else { "" };
                                    Value::String(format!("{}{}", name, suffix))
                                })
                                .collect();
                            return Value::Array(arr);
                        }
                    }
                }
                Value::Array(Vec::new())
            }
            "readFile" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Null,
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Null,
                    };
                    let manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive(&drive_name) {
                        if let Some(data) = drive.read_file(&filename) {
                            if let Ok(content) = core::str::from_utf8(&data) {
                                return Value::String(String::from(content));
                            }
                        }
                    }
                }
                Value::Null
            }
            "writeFile" => {
                if args.len() >= 3 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let content = match self.evaluate(&args[2]) {
                        Value::String(s) => s,
                        v => v.to_display(),
                    };
                    let mut manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive_mut(&drive_name) {
                        return Value::Bool(drive.write_file(&filename, content.as_bytes()));
                    }
                }
                Value::Bool(false)
            }
            "createFile" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let mut manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive_mut(&drive_name) {
                        return Value::Bool(drive.create_file(&filename));
                    }
                }
                Value::Bool(false)
            }
            "createDir" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let dirname = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let mut manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive_mut(&drive_name) {
                        return Value::Bool(drive.create_directory(&dirname));
                    }
                }
                Value::Bool(false)
            }
            "deleteFile" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let mut manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive_mut(&drive_name) {
                        return Value::Bool(drive.delete_file(&filename));
                    }
                }
                Value::Bool(false)
            }
            "fileExists" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive(&drive_name) {
                        return Value::Bool(drive.file_exists(&filename));
                    }
                }
                Value::Bool(false)
            }
            "fileSize" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Int(0),
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Int(0),
                    };
                    let manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive(&drive_name) {
                        if let Some((size, _is_dir)) = drive.get_file_info(&filename) {
                            return Value::Int(size as i64);
                        }
                    }
                }
                Value::Int(0)
            }
            "isDir" => {
                if args.len() >= 2 {
                    let drive_name = match self.evaluate(&args[0]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let filename = match self.evaluate(&args[1]) {
                        Value::String(s) => s,
                        _ => return Value::Bool(false),
                    };
                    let manager = DRIVE_MANAGER.lock();
                    if let Some(drive) = manager.get_drive(&drive_name) {
                        if let Some((_size, is_dir)) = drive.get_file_info(&filename) {
                            return Value::Bool(is_dir);
                        }
                    }
                }
                Value::Bool(false)
            }
            "arrayLen" => {
                if let Some(arg) = args.first() {
                    if let Value::Array(arr) = self.evaluate(arg) {
                        return Value::Int(arr.len() as i64);
                    }
                }
                Value::Int(0)
            }
            "arrayGet" => {
                if args.len() >= 2 {
                    let arr = self.evaluate(&args[0]);
                    let idx = self.evaluate(&args[1]).to_int() as usize;
                    if let Value::Array(arr) = arr {
                        if idx < arr.len() {
                            return arr[idx].clone();
                        }
                    }
                }
                Value::Null
            }
            "concat" => {
                let mut result = String::new();
                for arg in args {
                    result.push_str(&self.evaluate(arg).to_display());
                }
                Value::String(result)
            }
            _ => {
                // User-defined function
                if let Some(func) = self.functions.get(name).cloned() {
                    // Evaluate arguments
                    let arg_values: Vec<Value> = args.iter().map(|a| self.evaluate(a)).collect();
                    
                    // Bind parameters (save old values if they exist)
                    let mut saved_params: Vec<(String, Option<Value>)> = Vec::new();
                    for (param, value) in func.params.iter().zip(arg_values.iter()) {
                        saved_params.push((param.clone(), self.variables.get(param).cloned()));
                        self.variables.insert(param.clone(), value.clone());
                    }
                    
                    // Execute body
                    let mut result = Value::Null;
                    for stmt in &func.body {
                        if let Stmt::Return(Some(expr)) = stmt {
                            result = self.evaluate(expr);
                            break;
                        }
                        self.execute_stmt(stmt);
                    }
                    
                    // Restore only the parameter names to their previous values
                    for (param, old_value) in saved_params {
                        if let Some(v) = old_value {
                            self.variables.insert(param, v);
                        } else {
                            self.variables.remove(&param);
                        }
                    }
                    
                    result
                } else {
                    Value::Null
                }
            }
        }
    }
    
    /// Get variable value for text interpolation
    pub fn get_var(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }
    
    /// Interpolate {variables} in a string
    pub fn interpolate(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '{' {
                // Collect variable name
                let mut var_name = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '}' {
                        chars.next();
                        break;
                    }
                    var_name.push(chars.next().unwrap());
                }
                // Look up and insert value
                if let Some(val) = self.get_var(&var_name) {
                    result.push_str(&val.to_display());
                } else {
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                }
            } else {
                result.push(c);
            }
        }
        
        result
    }
    
    /// Take pending action (clears it)
    pub fn take_action(&mut self) -> ScriptAction {
        let action = self.pending_action.clone();
        self.pending_action = ScriptAction::None;
        action
    }
}

/// Simple script parser
struct ScriptParser<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> ScriptParser<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }
    
    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }
    
    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else if c == '/' {
                // Check for comments
                let next_pos = self.pos + 1;
                if next_pos < self.source.len() {
                    let next = self.source[next_pos..].chars().next();
                    if next == Some('/') {
                        // Line comment
                        while let Some(c) = self.peek() {
                            if c == '\n' { break; }
                            self.advance();
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    
    fn parse_identifier(&mut self) -> Option<String> {
        self.skip_whitespace();
        let start = self.pos;
        
        if let Some(c) = self.peek() {
            if !c.is_alphabetic() && c != '_' {
                return None;
            }
        } else {
            return None;
        }
        
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        
        if self.pos > start {
            Some(self.source[start..self.pos].to_string())
        } else {
            None
        }
    }
    
    fn parse_number(&mut self) -> Option<Value> {
        self.skip_whitespace();
        let start = self.pos;
        let mut has_dot = false;
        
        // Handle negative
        if self.peek() == Some('-') {
            self.advance();
        }
        
        while let Some(c) = self.peek() {
            if c.is_numeric() {
                self.advance();
            } else if c == '.' && !has_dot {
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }
        
        if self.pos > start {
            let num_str = &self.source[start..self.pos];
            if has_dot {
                num_str.parse::<f64>().ok().map(Value::Float)
            } else {
                num_str.parse::<i64>().ok().map(Value::Int)
            }
        } else {
            None
        }
    }
    
    fn parse_string(&mut self) -> Option<String> {
        self.skip_whitespace();
        
        let quote = self.peek()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        self.advance();
        
        let mut result = String::new();
        while let Some(c) = self.peek() {
            if c == quote {
                self.advance();
                return Some(result);
            }
            if c == '\\' {
                self.advance();
                if let Some(escaped) = self.advance() {
                    match escaped {
                        'n' => result.push('\n'),
                        't' => result.push('\t'),
                        _ => result.push(escaped),
                    }
                }
            } else {
                result.push(c);
                self.advance();
            }
        }
        
        Some(result)
    }
    
    fn consume(&mut self, expected: &str) -> bool {
        self.skip_whitespace();
        if self.source[self.pos..].starts_with(expected) {
            self.pos += expected.len();
            true
        } else {
            false
        }
    }
    
    fn parse_script(&mut self) -> Result<(Vec<Stmt>, Vec<Function>), ()> {
        let mut vars = Vec::new();
        let mut funcs = Vec::new();
        
        loop {
            self.skip_whitespace();
            if self.pos >= self.source.len() {
                break;
            }
            
            if self.consume("func") {
                if let Some(f) = self.parse_function() {
                    funcs.push(f);
                }
            } else if self.consume("var") {
                if let Some(stmt) = self.parse_var_decl() {
                    vars.push(stmt);
                }
            } else {
                // Skip unknown
                self.advance();
            }
        }
        
        Ok((vars, funcs))
    }
    
    fn parse_inline(&mut self) -> Result<Vec<Stmt>, ()> {
        let mut stmts = Vec::new();
        
        loop {
            self.skip_whitespace();
            if self.pos >= self.source.len() {
                break;
            }
            
            if let Some(stmt) = self.parse_statement() {
                stmts.push(stmt);
            } else {
                break;
            }
            
            // Optional semicolon
            self.consume(";");
        }
        
        Ok(stmts)
    }
    
    fn parse_var_decl(&mut self) -> Option<Stmt> {
        let name = self.parse_identifier()?;
        self.consume("=");
        let value = self.parse_expr()?;
        Some(Stmt::VarDecl { name, value })
    }
    
    fn parse_function(&mut self) -> Option<Function> {
        let name = self.parse_identifier()?;
        
        // Parse parameters
        self.consume("(");
        let mut params = Vec::new();
        loop {
            self.skip_whitespace();
            if self.consume(")") {
                break;
            }
            if let Some(param) = self.parse_identifier() {
                params.push(param);
            }
            self.consume(",");
        }
        
        // Parse body
        self.consume("{");
        let body = self.parse_block();
        self.consume("}");
        
        Some(Function { name, params, body })
    }
    
    fn parse_block(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        
        loop {
            self.skip_whitespace();
            if self.peek() == Some('}') || self.pos >= self.source.len() {
                break;
            }
            
            if let Some(stmt) = self.parse_statement() {
                stmts.push(stmt);
            } else {
                break;
            }
        }
        
        stmts
    }
    
    fn parse_statement(&mut self) -> Option<Stmt> {
        self.skip_whitespace();
        
        // Check for keywords
        if self.consume("if") {
            return self.parse_if();
        }
        if self.consume("while") {
            return self.parse_while();
        }
        if self.consume("return") {
            let expr = self.parse_expr();
            return Some(Stmt::Return(expr));
        }
        if self.consume("var") {
            return self.parse_var_decl();
        }
        
        // Try assignment or expression
        let saved_pos = self.pos;
        if let Some(name) = self.parse_identifier() {
            self.skip_whitespace();
            if self.consume("=") && !self.consume("=") {
                // Assignment (but not ==)
                self.pos -= 1; // Back up one since we consumed an extra =
                if let Some(value) = self.parse_expr() {
                    return Some(Stmt::Assign { name, value });
                }
            }
            // Reset and try as expression
            self.pos = saved_pos;
        }
        
        // Expression statement
        if let Some(expr) = self.parse_expr() {
            Some(Stmt::ExprStmt(expr))
        } else {
            None
        }
    }
    
    fn parse_if(&mut self) -> Option<Stmt> {
        let condition = self.parse_expr()?;
        
        self.consume("{");
        let then_block = self.parse_block();
        self.consume("}");
        
        let else_block = if self.consume("else") {
            self.consume("{");
            let block = self.parse_block();
            self.consume("}");
            Some(block)
        } else {
            None
        };
        
        Some(Stmt::If { condition, then_block, else_block })
    }
    
    fn parse_while(&mut self) -> Option<Stmt> {
        let condition = self.parse_expr()?;
        
        self.consume("{");
        let body = self.parse_block();
        self.consume("}");
        
        Some(Stmt::While { condition, body })
    }
    
    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_or()
    }
    
    fn parse_or(&mut self) -> Option<Expr> {
        let mut left = self.parse_and()?;
        
        while self.consume("||") {
            let right = self.parse_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOperator::Or,
                right: Box::new(right),
            };
        }
        
        Some(left)
    }
    
    fn parse_and(&mut self) -> Option<Expr> {
        let mut left = self.parse_equality()?;
        
        while self.consume("&&") {
            let right = self.parse_equality()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOperator::And,
                right: Box::new(right),
            };
        }
        
        Some(left)
    }
    
    fn parse_equality(&mut self) -> Option<Expr> {
        let mut left = self.parse_comparison()?;
        
        loop {
            let op = if self.consume("==") {
                BinOperator::Eq
            } else if self.consume("!=") {
                BinOperator::Ne
            } else {
                break;
            };
            
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Some(left)
    }
    
    fn parse_comparison(&mut self) -> Option<Expr> {
        let mut left = self.parse_additive()?;
        
        loop {
            let op = if self.consume("<=") {
                BinOperator::Le
            } else if self.consume(">=") {
                BinOperator::Ge
            } else if self.consume("<") {
                BinOperator::Lt
            } else if self.consume(">") {
                BinOperator::Gt
            } else {
                break;
            };
            
            let right = self.parse_additive()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Some(left)
    }
    
    fn parse_additive(&mut self) -> Option<Expr> {
        let mut left = self.parse_multiplicative()?;
        
        loop {
            let op = if self.consume("+") {
                BinOperator::Add
            } else if self.consume("-") {
                BinOperator::Sub
            } else {
                break;
            };
            
            let right = self.parse_multiplicative()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Some(left)
    }
    
    fn parse_multiplicative(&mut self) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        
        loop {
            let op = if self.consume("*") {
                BinOperator::Mul
            } else if self.consume("/") {
                BinOperator::Div
            } else if self.consume("%") {
                BinOperator::Mod
            } else {
                break;
            };
            
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Some(left)
    }
    
    fn parse_unary(&mut self) -> Option<Expr> {
        if self.consume("!") {
            let expr = self.parse_unary()?;
            return Some(Expr::UnaryOp {
                op: UnaryOperator::Not,
                expr: Box::new(expr),
            });
        }
        if self.consume("-") {
            // Check if it's a negative number
            if self.peek().map(|c| c.is_numeric()).unwrap_or(false) {
                self.pos -= 1; // Put the - back
                if let Some(val) = self.parse_number() {
                    return Some(Expr::Literal(val));
                }
            }
            let expr = self.parse_unary()?;
            return Some(Expr::UnaryOp {
                op: UnaryOperator::Neg,
                expr: Box::new(expr),
            });
        }
        
        self.parse_primary()
    }
    
    fn parse_primary(&mut self) -> Option<Expr> {
        self.skip_whitespace();
        
        // Parenthesized expression
        if self.consume("(") {
            let expr = self.parse_expr()?;
            self.consume(")");
            return Some(expr);
        }
        
        // Boolean literals
        if self.consume("true") {
            return Some(Expr::Literal(Value::Bool(true)));
        }
        if self.consume("false") {
            return Some(Expr::Literal(Value::Bool(false)));
        }
        if self.consume("null") {
            return Some(Expr::Literal(Value::Null));
        }
        
        // String literal
        if self.peek() == Some('"') || self.peek() == Some('\'') {
            if let Some(s) = self.parse_string() {
                return Some(Expr::Literal(Value::String(s)));
            }
        }
        
        // Number literal
        if self.peek().map(|c| c.is_numeric() || c == '-').unwrap_or(false) {
            if let Some(val) = self.parse_number() {
                return Some(Expr::Literal(val));
            }
        }
        
        // Identifier or function call
        if let Some(name) = self.parse_identifier() {
            self.skip_whitespace();
            
            // Check for function call
            if self.consume("(") {
                let mut args = Vec::new();
                loop {
                    self.skip_whitespace();
                    if self.consume(")") {
                        break;
                    }
                    if let Some(arg) = self.parse_expr() {
                        args.push(arg);
                    }
                    self.consume(",");
                }
                return Some(Expr::Call { name, args });
            }
            
            return Some(Expr::Var(name));
        }
        
        None
    }
}
