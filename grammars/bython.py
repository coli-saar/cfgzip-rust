
from grammars.base import GrammarSpec


base_nonterminal_str = r"""
root ::= program ticks
program ::= "" | ws-nl statement program ws-nl
ticks ::= T0
ws-req ::= T1
ws-nl ::= "" | T2
statement ::= compound-stmt | simple-stmt T3 | comment
comment ::= T51
simple-stmt ::= assignment | print-call | break-stmt | continue-stmt | pass-stmt | return-stmt | expression | import-statement
import-statement ::= T50 identifier import-statement-rec
import-statement-rec ::= "" | T40 identifier import-statement-rec
compound-stmt ::= if-stmt | for-stmt | while-stmt | func-def
block ::= T4 program ws-nl T5 ws-nl
if-stmt ::= T6 ws-req expression block elif-clause
elif-clause ::= "" | T7 ws-req expression block elif-clause | T8 block
while-stmt ::= T9 ws-req expression block
for-stmt ::= T10 ws-req identifier ws-req T11 ws-req expression block
func-def ::= T12 ws-req identifier func-args block
func-args ::= T13 expr-list T14
assignment ::= identifier T15 expression
print-call ::= T16 T13 expr-list T14
break-stmt ::= T17
continue-stmt ::= T18
pass-stmt ::= T19
return-stmt ::= T20 ws-req expression
expr-list ::= "" | expression | expression T21 expr-list
expression ::= expression expression-rhs | T29 expression | T37 ws-req expression | atom | T13 expression T14
expression-rhs ::= binop-rhs | ws-req binop-word-rhs | ws-req T6 ws-req expression ws-req T8 ws-req expression | trailer-rec  
binop-rhs ::= T22 expression | T23 expression | T24 expression | T25 expression | T26 expression | T27 expression | T28 expression | T29 expression | T30 expression | T31 expression | T32 expression | T33 expression | T34 expression | T35 expression | T47 expression
binop-word-rhs ::= T36 ws-req T37 ws-req expression | T36 ws-req expression | T37 ws-req T11 ws-req expression | T11 ws-req expression
trailer-rec ::= "" | trailer trailer-rec
trailer ::= func-args | T38 expression T39 | T40 identifier
atom ::= identifier | number | string | T43 | tuple-literal | list-literal
tuple-literal ::= T13 tuple-literal-rhs
tuple-literal-rhs ::= T14 | expression T21 expr-list T14
list-literal ::= T38 expr-list T39
identifier ::= T41
number ::= float-number | integer
integer ::= T46
float-number ::= integer T40 integer | T40 integer | integer T40
string ::= T48 | T49
"""
xgrammar_terminal_str = r"""
T0 ::= [ \t]* "```"
T1 ::= [ \t]+
T2 ::= [ \t\n]+
T3 ::= [ \t]* ";"
T4 ::= [ \t]* "{"
T5 ::= [ \t]* "}"
T6 ::= [ \t]* "if"
T7 ::= [ \t]* "elif"
T8 ::= [ \t]* "else"
T9 ::= [ \t]* "while"
T10 ::= [ \t]* "for"
T11 ::= [ \t]* "in"
T12 ::= [ \t]* "def"
T13 ::= [ \t]* "("
T14 ::= [ \t]* ")"
T15 ::= [ \t]* "="
T16 ::= [ \t]* "print"
T17 ::= [ \t]* "break"
T18 ::= [ \t]* "continue"
T19 ::= [ \t]* "pass"
T20 ::= [ \t]* "return"
T21 ::= [ \t]* ","
T22 ::= [ \t]* "<="
T23 ::= [ \t]* ">="
T24 ::= [ \t]* "!="
T25 ::= [ \t]* "=="
T26 ::= [ \t]* "<"
T27 ::= [ \t]* ">"
T28 ::= [ \t]* "+"
T29 ::= [ \t]* "-"
T30 ::= [ \t]* "*"
T31 ::= [ \t]* "/"
T32 ::= [ \t]* "//"
T33 ::= [ \t]* "%"
T34 ::= [ \t]* "&"
T35 ::= [ \t]* "**"
T36 ::= [ \t]* "is"
T37 ::= [ \t]* "not"
T38 ::= [ \t]* "["
T39 ::= [ \t]* "]"
T40 ::= [ \t]* "."
T41 ::= [ \t]* [a-zA-Z_] [a-zA-Z0-9_]*
T43 ::= [ \t]* ("True" | "False" | "None")
T46 ::= [ \t]* [0-9]+
T47 ::= [ \t]* "|"
T48 ::= [ \t]* ["] ([^\n\"\\`]* ([\\] [^\n\\`] [^\n\"\\`]*)*) ["]
T49 ::= [ \t]* "'" ([^\n'\\`]* ([\\] [^\n\\`] [^\n'\\`]*)*) "'"
T50 ::= [ \t]* "import" [ \t]+
T51 ::= [ \t]* "#" [^\n]+ [\n]+
"""
preproc_terminal_str = r"""
T0 ::= [ \t]*```
T1 ::= [ \t]+
T2 ::= [ \t\n]+
T3 ::= [ \t]*;
T4 ::= [ \t]*\{
T5 ::= [ \t]*\}
T6 ::= [ \t]*if
T7 ::= [ \t]*elif
T8 ::= [ \t]*else
T9 ::= [ \t]*while
T10 ::= [ \t]*for
T11 ::= [ \t]*in
T12 ::= [ \t]*def
T13 ::= [ \t]*\(
T14 ::= [ \t]*\)
T15 ::= [ \t]*=
T16 ::= [ \t]*print
T17 ::= [ \t]*break
T18 ::= [ \t]*continue
T19 ::= [ \t]*pass
T20 ::= [ \t]*return
T21 ::= [ \t]*,
T22 ::= [ \t]*<=
T23 ::= [ \t]*>=
T24 ::= [ \t]*!=
T25 ::= [ \t]*==
T26 ::= [ \t]*<
T27 ::= [ \t]*>
T28 ::= [ \t]*\+
T29 ::= [ \t]*\-
T30 ::= [ \t]*\*
T31 ::= [ \t]*/
T32 ::= [ \t]*//
T33 ::= [ \t]*%
T34 ::= [ \t]*\&
T35 ::= [ \t]*\*\*
T36 ::= [ \t]*is
T37 ::= [ \t]*not
T38 ::= [ \t]*\[
T39 ::= [ \t]*\]
T40 ::= [ \t]*\.
T41 ::= [ \t]*[a-zA-Z_][a-zA-Z0-9_]*
T43 ::= [ \t]*(True|False|None)
T46 ::= [ \t]*[0-9]+
T47 ::= [ \t]*\|
T48 ::= [ \t]*"([^\n"\\`]*(\\[^\n\\`][^\n"\\`]*)*)"
T49 ::= [ \t]*'([^\n'\\`]*(\\[^\n\\`][^\n'\\`]*)*)'
T50 ::= [ \t]*import[ \t]+
T51 ::= [ \t]*\#[^\n]+\n+
"""

llguidance_str = r"""
// =====================================================================
// Lark/llguidance translation of xgrammar_str.
//
// Right-recursive rules from the GBNF source were rewritten -- llguidance's
// Earley parser hits item limits on right recursion (per llguidance_syntax.md
// "Recursive rules" section). Mappings used:
//
//   GBNF "" | A X        ->  Lark a*               (right-rec list-or-empty)
//     applied to: import-statement-rec (inlined as (T40 identifier)*)
//   GBNF "" | ws-nl statement program ws-nl
//     ->  Lark program: (ws-nl statement)* ws-nl
//     Same language: each statement carries surrounding optional whitespace;
//     adjacent ws-nl's collapse since T2 = [ \t\n]+ is itself idempotent
//     under concatenation modulo nullability.
//   GBNF "" | T7 ws-req expression block elif-clause | T8 block
//     ->  Lark elif-clause: (T7 ws-req expression block)* (T8 block)?
//   GBNF "" | expression | expression T21 expr-list
//     ->  Lark expr-list: (expression (T21 expression)*)?
//   GBNF "" | trailer trailer-rec
//     ->  Lark trailer-rec: trailer+
//     The empty derivation only appeared via expression-rhs -> trailer-rec -> e,
//     which collapses expression -> expression and adds nothing to the language.
//   GBNF expression unary prefixes (T29 expression, T37 ws-req expression,
//     both right-recursive) flattened to a Kleene-star prefix on the base term:
//       expression: (T29 | T37 ws-req)* expression-base expression-rhs*
//       expression-base: atom | T13 expression T14
//     The left-recursive `expression expression-rhs` is folded into the
//     postfix expression-rhs* so the surface remains left-associative.
//
// Whitespace handling matches the GBNF: each terminal carries its own
// optional [ \t]* prefix (or [ \t\n]+ for T2); ws-req (T1) and ws-nl (T2?)
// are preserved verbatim as hard separators in rule bodies. No %ignore.
// =====================================================================

start: program ticks
program: (ws-nl statement)* ws-nl
ticks: T0
ws-req: T1
ws-nl: T2?
statement: compound-stmt | simple-stmt T3 | comment
comment: T51
simple-stmt: assignment | print-call | break-stmt | continue-stmt | pass-stmt | return-stmt | expression | import-statement
import-statement: T50 identifier (T40 identifier)*
compound-stmt: if-stmt | for-stmt | while-stmt | func-def
block: T4 program ws-nl T5 ws-nl
if-stmt: T6 ws-req expression block elif-clause
elif-clause: (T7 ws-req expression block)* (T8 block)?
while-stmt: T9 ws-req expression block
for-stmt: T10 ws-req identifier ws-req T11 ws-req expression block
func-def: T12 ws-req identifier func-args block
func-args: T13 expr-list T14
assignment: identifier T15 expression
print-call: T16 T13 expr-list T14
break-stmt: T17
continue-stmt: T18
pass-stmt: T19
return-stmt: T20 ws-req expression
expr-list: (expression (T21 expression)*)?
expression: (T29 | T37 ws-req)* expression-base expression-rhs*
expression-base: atom | T13 expression T14
expression-rhs: binop-rhs | ws-req binop-word-rhs | ws-req T6 ws-req expression ws-req T8 ws-req expression | trailer-rec
binop-rhs: T22 expression | T23 expression | T24 expression | T25 expression | T26 expression | T27 expression | T28 expression | T29 expression | T30 expression | T31 expression | T32 expression | T33 expression | T34 expression | T35 expression | T47 expression
binop-word-rhs: T36 ws-req T37 ws-req expression | T36 ws-req expression | T37 ws-req T11 ws-req expression | T11 ws-req expression
trailer-rec: trailer+
trailer: func-args | T38 expression T39 | T40 identifier
atom: identifier | number | string | T43 | tuple-literal | list-literal
tuple-literal: T13 tuple-literal-rhs
tuple-literal-rhs: T14 | expression T21 expr-list T14
list-literal: T38 expr-list T39
identifier: T41
number: float-number | integer
integer: T46
float-number: integer T40 integer | T40 integer | integer T40
string: T48 | T49

T0: /[ \t]*/ "```"
T1: /[ \t]+/
T2: /[ \t\n]+/
T3: /[ \t]*/ ";"
T4: /[ \t]*/ "{"
T5: /[ \t]*/ "}"
T6: /[ \t]*/ "if"
T7: /[ \t]*/ "elif"
T8: /[ \t]*/ "else"
T9: /[ \t]*/ "while"
T10: /[ \t]*/ "for"
T11: /[ \t]*/ "in"
T12: /[ \t]*/ "def"
T13: /[ \t]*/ "("
T14: /[ \t]*/ ")"
T15: /[ \t]*/ "="
T16: /[ \t]*/ "print"
T17: /[ \t]*/ "break"
T18: /[ \t]*/ "continue"
T19: /[ \t]*/ "pass"
T20: /[ \t]*/ "return"
T21: /[ \t]*/ ","
T22: /[ \t]*/ "<="
T23: /[ \t]*/ ">="
T24: /[ \t]*/ "!="
T25: /[ \t]*/ "=="
T26: /[ \t]*/ "<"
T27: /[ \t]*/ ">"
T28: /[ \t]*/ "+"
T29: /[ \t]*/ "-"
T30: /[ \t]*/ "*"
T31: /[ \t]*/ "/"
T32: /[ \t]*/ "//"
T33: /[ \t]*/ "%"
T34: /[ \t]*/ "&"
T35: /[ \t]*/ "**"
T36: /[ \t]*/ "is"
T37: /[ \t]*/ "not"
T38: /[ \t]*/ "["
T39: /[ \t]*/ "]"
T40: /[ \t]*/ "."
T41: /[ \t]*[a-zA-Z_][a-zA-Z0-9_]*/
T43: /[ \t]*/ ("True" | "False" | "None")
T46: /[ \t]*[0-9]+/
T47: /[ \t]*/ "|"
T48: /[ \t]*"([^\n"\\`]*(\\[^\n\\`][^\n"\\`]*)*)"/
T49: /[ \t]*'([^\n'\\`]*(\\[^\n'\\`][^\n'\\`]*)*)'/
T50: /[ \t]*import[ \t]+/
T51: /[ \t]*#[^\n]+\n+/
""".strip()

grammar_spec = GrammarSpec(
    name='bython',
    xgrammar_str=f'{base_nonterminal_str}\n{xgrammar_terminal_str}',
    preprocessing_str=f'{base_nonterminal_str}\nTERMINALS:\n{preproc_terminal_str}',
    llguidance_str=llguidance_str
)