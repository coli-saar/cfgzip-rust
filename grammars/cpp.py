
from grammars.base import GrammarSpec


base_nonterminal_str = """
root ::= Includes Program | Includes Program T0
Args ::= Expression | Expression T15 Args
Binop ::= Expression BinopRHS
BinopRHS ::= T20 T20 Expression | T3 T3 Expression | T3 T3 T23 Expression | T39 T23 Expression | T4 T4 Expression | T4 T4 T23 Expression | T42 Expression | T42 T23 Expression | T44 T44 Expression | T8 T43 T8 Expression
Body ::= "" | BodyContent Body
BodyContent ::= Statement | T21 | VariableDef
BreakStatement ::= T29 T10
Capture ::= CopyCapture | RefCapture
CaptureList ::= Capture | Capture T15 CaptureList
Case ::= T36 T8 Expression T18 Body
CaseList ::= Case | Case CaseList
CharLiteral ::= T48
Comment ::= T21 | T22
ContinueStatement ::= T28 T10
CopyCapture ::= Name
CopyCaptureList ::= CopyCapture | CopyCapture T15 CopyCaptureList
ElsePart ::= "" | T32 StatementOrGroupedStatement
Expression ::= Expression ExpressionRHS | GroupedExpression | LambdaExpression | Literal | Name | TypeCast | Unop
ExpressionList ::= Expression | Expression T15 ExpressionList
ExpressionRHS ::= BinopRHS | FunctionCallRHS | T24 Expression T25 | T38 T4 Name | T45 Expression T18 Expression | T5 Name | UnopRHS
ExpressionStmt ::= OptExpression T10
ForStatement ::= T33 ForStatementRHS StatementOrGroupedStatement
ForStatementRHS ::= T13 ExpressionStmt OptExpression T10 OptExpression T14 | T13 Type T8 InitElem T18 Expression T14 | T13 VariableDef OptExpression T10 OptExpression T14
FullCaptureList ::= CaptureList | T20 T15 OptCopyCaptureList | T23 T15 OptRefCaptureList
FunctionCallRHS ::= OptArgs | T3 TypeList T4 OptArgs
FunctionDef ::= Head GroupedStatement
GroupedExpression ::= T13 Expression T14
GroupedStatement ::= T26 Body T27
Head ::= Type T8 Name OptParams
IfStatement ::= T31 T13 Expression T14 StatementOrGroupedStatement ElsePart
Include ::= Comment | T1 T2 T3 IncludePath T5 T6 T4 | T1 T2 T3 Name T4 | T11 T8 Type T8 Name T10 | T7 T8 T9 T8 Name T10
IncludePath ::= Name | Name T12 IncludePath
Includes ::= "" | Include Includes
InitElem ::= Name | Name InitElemRHS
InitElemRHS ::= OptArgs | T23 Expression | T24 Expression T25 | T24 Expression T25 T23 Expression
InitList ::= InitElem | InitElem T15 InitList
LambdaExpression ::= T24 OptCaptureList T25 GroupedStatement | T24 OptCaptureList T25 OptParams GroupedStatement
Literal ::= CharLiteral | Number | StringLiteral | VectorLiteral
Name ::= T18 T18 Name | T49 | T49 T18 T18 Name
Number ::= T46
OptArgs ::= T13 Args T14 | T13 T14
OptCaptureList ::= "" | FullCaptureList
OptCopyCaptureList ::= "" | CopyCaptureList
OptDefault ::= "" | T37 T18 Body
OptExpression ::= "" | Expression
OptExpressionList ::= "" | ExpressionList
OptParams ::= T13 Params T14 | T13 T14
OptRefCaptureList ::= "" | RefCaptureList
Param ::= Type T8 Name
Params ::= Param | Param T15 Params
Program ::= TLD | TLD Program
RefCapture ::= T20 Name
RefCaptureList ::= RefCapture | RefCapture T15 RefCaptureList
ReturnStatement ::= T30 T10 | T30 T8 Expression T10
Statement ::= BreakStatement | ContinueStatement | ExpressionStmt | ForStatement | IfStatement | ReturnStatement | SwitchStatement | WhileStatement
StatementOrGroupedStatement ::= GroupedStatement | Statement
StringLiteral ::= T47
SwitchStatement ::= T35 T13 Expression T14 T26 CaseList OptDefault T27
TLD ::= Comment | FunctionDef
Type ::= Name | T16 T8 Name | T17 T8 Type | Type T18 T18 Type | Type T19 | Type T20 | Type T3 TypeList T4
TypeCast ::= T13 Type T14 Expression
TypeList ::= Type | Type T15 TypeList
Unop ::= T19 Expression | T20 Expression | T38 Expression | T38 T38 Expression | T39 Expression | T40 T40 Expression | T41 T8 Expression
UnopRHS ::= T38 T38 | T40 T40
VariableDef ::= Type T8 InitList T10
VectorLiteral ::= T26 OptExpressionList T27
WhileStatement ::= T34 T13 Expression T14 StatementOrGroupedStatement
""".strip()
xgrammar_terminal_str = r"""
T43 ::= [ \n\t]* ("and" | "or" | "xor")
T1 ::= [ \n\t]* "#"
T26 ::= [ \n\t]* "{"
T27 ::= [ \n\t]* "}"
T24 ::= [ \n\t]* "["
T25 ::= [ \n\t]* "]"
T13 ::= [ \n\t]* "("
T14 ::= [ \n\t]* ")"
T15 ::= [ \n\t]* ","
T23 ::= [ \n\t]* "="
T10 ::= [ \n\t]* ";"
T18 ::= [ \n\t]* ":"
T3 ::= [ \n\t]* "<"
T4 ::= [ \n\t]* ">"
T38 ::= [ \n\t]* "-"
T39 ::= [ \n\t]* "!"
T5 ::= [ \n\t]* "."
T40 ::= [ \n\t]* "+"
T20 ::= [ \n\t]* "&"
T19 ::= [ \n\t]* "*"
T12 ::= [ \n\t]* "/"
T45 ::= [ \n\t]* "?"
T33 ::= [ \n\t]* "for"
T31 ::= [ \n\t]* "if"
T34 ::= [ \n\t]* "while"
T30 ::= [ \n\t]* "return"
T32 ::= [ \n\t]* "else"
T29 ::= [ \n\t]* "break"
T28 ::= [ \n\t]* "continue"
T2 ::= [ \n\t]* "include"
T7 ::= [ \n\t]* "using"
T9 ::= [ \n\t]* "namespace"
T41 ::= [ \n\t]* "not"
T11 ::= [ \n\t]* "typedef"
T35 ::= [ \n\t]* "switch"
T36 ::= [ \n\t]* "case"
T37 ::= [ \n\t]* "default"
T17 ::= [ \n\t]* "const"
T46 ::= [ \n\t]* ((([0] | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)? | "." [0-9]+ | "." [0-9]+ [eE] [+-]? [0-9]+ | ([0] | [1-9] [0-9]*) [eE] [+-]? [0-9]+))
T49 ::= [ \n\t]* [a-zA-Z_] [a-zA-Z0-9_]*
T6 ::= [ \n\t]* ("h" | "hpp")
T16 ::= [ \n\t]* ("long" | "short" | "signed" | "unsigned")
T44 ::= [ \n\t]* "|"
T0 ::= [ \n\t]* "```"
T42 ::= [ \n\t]* ("+" | "-" | "*" | "/" | "%" | "<" | ">" | "&" | "|" | "=" | "^")
T47 ::= [ \n\t]* ["] ([^\n\"\\`]* ([\\] [^\n\\`] [^\n\"\\`]*)*) ["]
T48 ::= [ \n\t]* "'" ([^\\'] | [\\] [\\'tnvfrba]) "'"
T21 ::= [ \n\t]* "//" [^\n`/]+ "\n"
T22 ::= [ \n\t]* "/*" [^`/]+ "*/"
T8 ::= [ \n\t]
""".strip()
preproc_terminal_str = r"""
T43 ::= [ \n\t]*(and|or|xor)
T1 ::= [ \n\t]*\#
T26 ::= [ \n\t]*\{
T27 ::= [ \n\t]*\}
T24 ::= [ \n\t]*\[
T25 ::= [ \n\t]*\]
T13 ::= [ \n\t]*\(
T14 ::= [ \n\t]*\)
T15 ::= [ \n\t]*,
T23 ::= [ \n\t]*=
T10 ::= [ \n\t]*;
T18 ::= [ \n\t]*:
T3 ::= [ \n\t]*<
T4 ::= [ \n\t]*>
T38 ::= [ \n\t]*\-
T39 ::= [ \n\t]*!
T5 ::= [ \n\t]*\.
T40 ::= [ \n\t]*\+
T20 ::= [ \n\t]*\&
T19 ::= [ \n\t]*\*
T12 ::= [ \n\t]*/
T45 ::= [ \n\t]*\?
T33 ::= [ \n\t]*for
T31 ::= [ \n\t]*if
T34 ::= [ \n\t]*while
T30 ::= [ \n\t]*return
T32 ::= [ \n\t]*else
T29 ::= [ \n\t]*break
T28 ::= [ \n\t]*continue
T2 ::= [ \n\t]*include
T7 ::= [ \n\t]*using
T9 ::= [ \n\t]*namespace
T41 ::= [ \n\t]*not
T11 ::= [ \n\t]*typedef
T35 ::= [ \n\t]*switch
T36 ::= [ \n\t]*case
T37 ::= [ \n\t]*default
T17 ::= [ \n\t]*const
T46 ::= [ \n\t]*(((0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?|\.[0-9]+|\.[0-9]+[eE][+-]?[0-9]+|(0|[1-9][0-9]*)[eE][+-]?[0-9]+))
T49 ::= [ \n\t]*[a-zA-Z_][a-zA-Z0-9_]*
T6 ::= [ \n\t]*(h|hpp)
T16 ::= [ \n\t]*(long|short|signed|unsigned)
T44 ::= [ \n\t]*\|
T0 ::= [ \n\t]*```
T42 ::= [ \n\t]*[+\-*/%<>&|=^]
T47 ::= [ \n\t]*"([^\n"\\`]*(\\[^\n\\`][^\n"\\`]*)*)"
T48 ::= [ \n\t]*'([^\\']|[\\][\\'tnvfrba])'
T21 ::= [ \n\t]*//[^\n`/]+\n
T22 ::= [ \n\t]*/\*[^`/]+\*/
T8 ::= [ \n\t]
""".strip()

llguidance_str = r"""
// =====================================================================
// Lark/llguidance translation of xgrammar_str.
//
// Right-recursive rules from the GBNF source were rewritten -- llguidance's
// Earley parser hits item limits on right recursion (per llguidance_syntax.md
// "Recursive rules" section). Mappings used:
//
//   GBNF "" | A X        ->  Lark a*               (right-rec list-or-empty)
//     applied to: Body, Includes
//   GBNF A | A X         ->  Lark a+               (right-rec one-or-more)
//     applied to: Program (-> tld+), CaseList (-> case+)
//   GBNF A | A T A's     ->  Lark a (T a)*         (right-rec separator list)
//     applied to: Args, CaptureList, CopyCaptureList, RefCaptureList,
//                 ExpressionList, IncludePath, InitList, Params, TypeList
//   GBNF "" | X          ->  Lark x?               (optional)
//     applied to: ElsePart, OptCaptureList, OptCopyCaptureList,
//                 OptDefault, OptExpression, OptExpressionList,
//                 OptRefCaptureList; init-elem-rhs? in InitElem
//   GBNF T_a A T_b | T_a T_b -> Lark T_a a? T_b   (factored optional)
//     applied to: OptArgs, OptParams
//   GBNF Name = T18 T18 Name | T49 | T49 T18 T18 Name (right-rec on both arms)
//     ->  Lark name: (T18 T18)* T49 (T18 T18 T49)*
//     Same language: any number of leading "::" then identifier then "::ident" chain.
//
// Left-recursive rules (Expression, Type) are kept verbatim -- Earley
// handles those efficiently per the docs.
//
// Whitespace handling matches the GBNF: each terminal carries its own
// optional [ \n\t]* prefix; T8 (mandatory single whitespace) is preserved
// in rule bodies that use it as a hard separator. No %ignore.
// =====================================================================

start: includes program | includes program T0
args: expression (T15 expression)*
binop: expression binop-rhs
binop-rhs: T20 T20 expression | T3 T3 expression | T3 T3 T23 expression | T39 T23 expression | T4 T4 expression | T4 T4 T23 expression | T42 expression | T42 T23 expression | T44 T44 expression | T8 T43 T8 expression
body: body-content*
body-content: statement | T21 | variable-def
break-statement: T29 T10
capture: copy-capture | ref-capture
capture-list: capture (T15 capture)*
case: T36 T8 expression T18 body
case-list: case+
char-literal: T48
comment: T21 | T22
continue-statement: T28 T10
copy-capture: name
copy-capture-list: copy-capture (T15 copy-capture)*
else-part: (T32 statement-or-grouped-statement)?
expression: expression expression-rhs | grouped-expression | lambda-expression | literal | name | type-cast | unop
expression-list: expression (T15 expression)*
expression-rhs: binop-rhs | function-call-rhs | T24 expression T25 | T38 T4 name | T45 expression T18 expression | T5 name | unop-rhs
expression-stmt: opt-expression T10
for-statement: T33 for-statement-rhs statement-or-grouped-statement
for-statement-rhs: T13 expression-stmt opt-expression T10 opt-expression T14 | T13 type T8 init-elem T18 expression T14 | T13 variable-def opt-expression T10 opt-expression T14
full-capture-list: capture-list | T20 T15 opt-copy-capture-list | T23 T15 opt-ref-capture-list
function-call-rhs: opt-args | T3 type-list T4 opt-args
function-def: head grouped-statement
grouped-expression: T13 expression T14
grouped-statement: T26 body T27
head: type T8 name opt-params
if-statement: T31 T13 expression T14 statement-or-grouped-statement else-part
include: comment | T1 T2 T3 include-path T5 T6 T4 | T1 T2 T3 name T4 | T11 T8 type T8 name T10 | T7 T8 T9 T8 name T10
include-path: name (T12 name)*
includes: include*
init-elem: name init-elem-rhs?
init-elem-rhs: opt-args | T23 expression | T24 expression T25 | T24 expression T25 T23 expression
init-list: init-elem (T15 init-elem)*
lambda-expression: T24 opt-capture-list T25 grouped-statement | T24 opt-capture-list T25 opt-params grouped-statement
literal: char-literal | number | string-literal | vector-literal
name: (T18 T18)* T49 (T18 T18 T49)*
number: T46
opt-args: T13 args? T14
opt-capture-list: full-capture-list?
opt-copy-capture-list: copy-capture-list?
opt-default: (T37 T18 body)?
opt-expression: expression?
opt-expression-list: expression-list?
opt-params: T13 params? T14
opt-ref-capture-list: ref-capture-list?
param: type T8 name
params: param (T15 param)*
program: tld+
ref-capture: T20 name
ref-capture-list: ref-capture (T15 ref-capture)*
return-statement: T30 T10 | T30 T8 expression T10
statement: break-statement | continue-statement | expression-stmt | for-statement | if-statement | return-statement | switch-statement | while-statement
statement-or-grouped-statement: grouped-statement | statement
string-literal: T47
switch-statement: T35 T13 expression T14 T26 case-list opt-default T27
tld: comment | function-def
type: name | T16 T8 name | T17 T8 type | type T18 T18 type | type T19 | type T20 | type T3 type-list T4
type-cast: T13 type T14 expression
type-list: type (T15 type)*
unop: T19 expression | T20 expression | T38 expression | T38 T38 expression | T39 expression | T40 T40 expression | T41 T8 expression
unop-rhs: T38 T38 | T40 T40
variable-def: type T8 init-list T10
vector-literal: T26 opt-expression-list T27
while-statement: T34 T13 expression T14 statement-or-grouped-statement

T0: /[ \n\t]*/ "```"
T1: /[ \n\t]*/ "#"
T2: /[ \n\t]*/ "include"
T3: /[ \n\t]*/ "<"
T4: /[ \n\t]*/ ">"
T5: /[ \n\t]*/ "."
T6: /[ \n\t]*/ ("h" | "hpp")
T7: /[ \n\t]*/ "using"
T8: /[ \n\t]/
T9: /[ \n\t]*/ "namespace"
T10: /[ \n\t]*/ ";"
T11: /[ \n\t]*/ "typedef"
T12: /[ \n\t]*/ "/"
T13: /[ \n\t]*/ "("
T14: /[ \n\t]*/ ")"
T15: /[ \n\t]*/ ","
T16: /[ \n\t]*/ ("long" | "short" | "signed" | "unsigned")
T17: /[ \n\t]*/ "const"
T18: /[ \n\t]*/ ":"
T19: /[ \n\t]*/ "*"
T20: /[ \n\t]*/ "&"
T21: /[ \n\t]*\/\/[^\n`\/]+\n/
T22: /[ \n\t]*\/\*[^`\/]+\*\//
T23: /[ \n\t]*/ "="
T24: /[ \n\t]*/ "["
T25: /[ \n\t]*/ "]"
T26: /[ \n\t]*/ "{"
T27: /[ \n\t]*/ "}"
T28: /[ \n\t]*/ "continue"
T29: /[ \n\t]*/ "break"
T30: /[ \n\t]*/ "return"
T31: /[ \n\t]*/ "if"
T32: /[ \n\t]*/ "else"
T33: /[ \n\t]*/ "for"
T34: /[ \n\t]*/ "while"
T35: /[ \n\t]*/ "switch"
T36: /[ \n\t]*/ "case"
T37: /[ \n\t]*/ "default"
T38: /[ \n\t]*/ "-"
T39: /[ \n\t]*/ "!"
T40: /[ \n\t]*/ "+"
T41: /[ \n\t]*/ "not"
T42: /[ \n\t]*/ ("+" | "-" | "*" | "/" | "%" | "<" | ">" | "&" | "|" | "=" | "^")
T43: /[ \n\t]*/ ("and" | "or" | "xor")
T44: /[ \n\t]*/ "|"
T45: /[ \n\t]*/ "?"
T46: /[ \n\t]*((0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?|\.[0-9]+|\.[0-9]+[eE][+-]?[0-9]+|(0|[1-9][0-9]*)[eE][+-]?[0-9]+)/
T47: /[ \n\t]*"([^\n"\\`]*(\\[^\n\\`][^\n"\\`]*)*)"/
T48: /[ \n\t]*'([^\\']|\\[\\'tnvfrba])'/
T49: /[ \n\t]*[a-zA-Z_][a-zA-Z0-9_]*/
""".strip()

grammar_spec = GrammarSpec(
    name='cpp',
    xgrammar_str=f'{base_nonterminal_str}\n{xgrammar_terminal_str}',
    llguidance_str=llguidance_str,
    preprocessing_str=f'{base_nonterminal_str}\nTERMINALS:\n{preproc_terminal_str}'
)


