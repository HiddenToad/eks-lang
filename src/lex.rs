use crate::utils::peek_while;

//Span represents a token's lexical position
//Start -> End is a char range
//Line : Col is line + column num
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize, 
    pub col: usize,  
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Self {
            start,
            end,
            line,
            col,
        }
    }
}

//A token which contains information about
//its lexical position in code.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

impl SpannedToken {
    pub fn new(token: Token, span: Span) -> Self {
        Self { token, span }
    }
}

//Allow SpannedToken to deref to Token
//This is just for convenience, but makes
//SpannedToken feel more "related" to Token
//rather than just composed of it.
impl std::ops::Deref for SpannedToken {
    type Target = Token;
    fn deref(&self) -> &Self::Target {
        &self.token
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    //User-defined identifier [ex: my_comp]
    Ident(String),
    
    //Integer literal [ex: 2]
    Int(i64),
    
    //Floating-point literal [ex: 12.5]
    Float(f64),
    
    //String literal (quoted) [ex: "hello"]
    StringLiteral(String),

    // Single char symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Assign, // =
    Colon,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Lt,
    Gt,
    Not, //!

    // Two char symbols
    Eq, //==
    Neq, //!=
    Lte, //<=
    Gte, //>=
    And, //&&
    Or, //||

    // Keywords
    Comp,
    Ent,
    Fun,
    Sys,
    Let,
    IntType,
    FloatType,
    StringType,
    VoidType,
    BoolType,
    Return,
    True,
    False,

    EOF, //special token to mark end of input
}

//Types. Custom is user-defined (comp)
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    String,
    Void,
    Bool,
    Custom(String),
}

// Helper to update line/col
//Increments the line# when newline encountered
fn update_state_for_str(state: &mut (usize, usize), s: &str) {
    for c in s.chars() {
        if c == '\n' {
            state.0 += 1;
            state.1 = 1;
        } else {
            state.1 += 1;
        }
    }
}

pub fn lex(text: String) -> Vec<SpannedToken> {
    let mut tokens = vec![];
    let mut chars = text.char_indices().peekable();

    //(line, col)
    let mut state = (1usize, 1usize);

    
    while let Some(&(idx, c)) = chars.peek() {
        //whitespace is ignored semantically
        //just increments Span values
        if c.is_whitespace() {
            chars.next();
            if c == '\n' {
                state.0 += 1;
                state.1 = 1;
            } else {
                state.1 += 1;
            }
            continue;
        }

        let start = idx;
        let start_line = state.0;
        let start_col = state.1;

        //encountered string literal
        if c == '"' {
            chars.next(); //eat open quote
            state.1 += 1; //record opening quote
            let s = peek_while(&mut chars, |c| c.1 != '"'); //eat until closing quote
            update_state_for_str(&mut state, &s); 
            chars.next(); //eat close quote
            state.1 += 1; //record closing quote

            let len = s.len();
            tokens.push(SpannedToken::new(
                Token::StringLiteral(s),
                Span::new(start, start + len + 2, start_line, start_col),
            ));
        } else if c.is_ascii_digit() || c == '.' //allows leading decimal{
            let num_str = peek_while(&mut chars, |c| c.1.is_ascii_digit() || c.1 == '.');
            update_state_for_str(&mut state, &num_str);
            let span = Span::new(start, start + num_str.len(), start_line, start_col);

            if num_str.contains('.') {
                //float literal
                tokens.push(SpannedToken::new(
                    Token::Float(num_str.parse().expect("Invalid float")),
                    span,
                ));
            } else {
                // int literal
                tokens.push(SpannedToken::new(
                    Token::Int(num_str.parse().expect("Invalid int")),
                    span,
                ));
            }
        } else if c.is_alphabetic() || c == '_' { //ident or keyword
            let ident = peek_while(&mut chars, |c| c.1.is_alphanumeric() || c.1 == '_');
            update_state_for_str(&mut state, &ident);
            let span = Span::new(start, start + ident.len(), start_line, start_col);

            let token = match ident.as_str() {
                "comp" => Token::Comp,
                "ent" => Token::Ent,
                "fun" => Token::Fun,
                "sys" => Token::Sys,
                "let" => Token::Let,
                "int" => Token::IntType,
                "float" => Token::FloatType,
                "string" => Token::StringType,
                "void" => Token::VoidType,
                "bool" => Token::BoolType,
                "return" => Token::Return,
                "true" => Token::True,
                "false" => Token::False,
                _ => Token::Ident(ident),
            };
            tokens.push(SpannedToken::new(token, span));
        }
        //Two-character operators
        else if c == '=' { //distinguish between == and =
            chars.next();
            if matches!(chars.peek(), Some((_, '='))) {
                chars.next();
                state.1 += 2;
                tokens.push(SpannedToken::new( //==
                    Token::Eq,
                    Span::new(start, start + 2, start_line, start_col),
                ));
            } else {
                state.1 += 1;
                tokens.push(SpannedToken::new( //=
                    Token::Assign,
                    Span::new(start, start + 1, start_line, start_col),
                ));
            }
        } else if c == '!' {//distinguish between ! and !=
            chars.next();
            if matches!(chars.peek(), Some((_, '='))) {
                chars.next();
                state.1 += 2;
                tokens.push(SpannedToken::new( //!=
                    Token::Neq,
                    Span::new(start, start + 2, start_line, start_col),
                ));
            } else {
                state.1 += 1;
                tokens.push(SpannedToken::new( //!
                    Token::Not,
                    Span::new(start, start + 1, start_line, start_col),
                ));
            }
        } else if c == '<' { //distinguish btwn < and <=
            chars.next();
            if matches!(chars.peek(), Some((_, '='))) {
                chars.next();
                state.1 += 2;
                tokens.push(SpannedToken::new( //<=
                    Token::Lte,
                    Span::new(start, start + 2, start_line, start_col),
                ));
            } else {
                state.1 += 1;
                tokens.push(SpannedToken::new(//<
                    Token::Lt,
                    Span::new(start, start + 1, start_line, start_col),
                ));
            }
        } else if c == '>' { //distinguish between > and >=
            chars.next();
            if matches!(chars.peek(), Some((_, '='))) {
                chars.next();
                state.1 += 2;
                tokens.push(SpannedToken::new(//>=
                    Token::Gte,
                    Span::new(start, start + 2, start_line, start_col),
                ));
            } else {
                state.1 += 1;
                tokens.push(SpannedToken::new(//>
                    Token::Gt,
                    Span::new(start, start + 1, start_line, start_col),
                ));
            }
        } else if c == '&' { //&&
            chars.next();
            if matches!(chars.peek(), Some((_, '&'))) {
                chars.next();
                state.1 += 2;
                tokens.push(SpannedToken::new(
                    Token::And,
                    Span::new(start, start + 2, start_line, start_col),
                ));
            } else {
                panic!(
                    "Unexpected character '&' at line {}, column {}",
                    start_line, start_col
                );
            }
        } else if c == '|' { //||
            chars.next();
            if matches!(chars.peek(), Some((_, '|'))) {
                chars.next();
                state.1 += 2;
                tokens.push(SpannedToken::new(
                    Token::Or,
                    Span::new(start, start + 2, start_line, start_col),
                ));
            } else {
                panic!(
                    "Unexpected character '|' at line {}, column {}",
                    start_line, start_col
                );
            }
        }
        // Single-character tokens
        else {
            chars.next();
            state.1 += 1;
            let tok = match c {
                '(' => Token::LParen,
                ')' => Token::RParen,
                '{' => Token::LBrace,
                '}' => Token::RBrace,
                '[' => Token::LBracket,
                ']' => Token::RBracket,
                ';' => Token::Semicolon,
                ',' => Token::Comma,
                ':' => Token::Colon,
                '.' => Token::Dot,
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                _ => panic!(
                    "Unexpected character '{}' at line {}, column {}",
                    c, start_line, start_col
                ),
            };
            tokens.push(SpannedToken::new(
                tok,
                Span::new(start, start + 1, start_line, start_col),
            ));
        }
    }

    //EOF token uses the final state
    //This way if there's an "unexpected EOF" error,
    //It will correctly point to the end of the file.
    tokens.push(SpannedToken::new(
        Token::EOF,
        Span::new(text.len(), text.len(), state.0, state.1),
    ));
    tokens
}
