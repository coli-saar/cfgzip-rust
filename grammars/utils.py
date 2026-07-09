

import re
from typing import Dict, List


def ebnf_to_regex(ebnf_string: str) -> str:
    return _ebnf_to_regex_rec(rf'({ebnf_string})')[1:-1]


def _ebnf_to_regex_rec(ebnf_string: str) -> str:
    ebnf_string = ebnf_string.strip()

    if len(ebnf_string) < 2 or (ebnf_string[0] == '[' and ebnf_string[-1] == ']'):
        return ebnf_string.replace('\\"', '"').replace("\\'", "'")
    if ebnf_string[0] == ebnf_string[-1] == '"':
        in_str = ebnf_string[1:-1].replace('\\"', '"').replace("\\'", "'")
        out_str = re.escape(in_str).replace('\\"', '"').replace("\\'", "'")
        out_str = out_str.replace('\\ ', ' ')

        return out_str
    if ebnf_string[0] == '(' and ebnf_string[-1] == ')':
        groups = _extract_groups(ebnf_string)

        return f'({"".join(map(_ebnf_to_regex_rec, groups))})'

    raise Exception


def _extract_groups(ebnf_string: str, match_nonterminals: bool = False) -> List[str]:
    match_bracket, match_string, match_nt, paren_cnt, esc_mode = False, False, False, 0, False
    ebnf_string, groups = ebnf_string[1:-1], []

    for i, char in enumerate(ebnf_string):
        if match_nt and not re.match(r'[a-zA-Z0-9_]', char):
            match_nt = False
            groups[-1] = ebnf_string[groups[-1]:i]
        if char == '\\':
            esc_mode = not esc_mode
        elif esc_mode:
            esc_mode = False
        elif match_string:
            if char == '"':
                groups[-1] = ebnf_string[groups[-1]:(i + 1)]
                match_string = False
        elif match_bracket:
            if char == ']':
                groups[-1] = ebnf_string[groups[-1]:(i + 1)]
                match_bracket = False
        elif paren_cnt > 0:
            if char == ')':
                paren_cnt -= 1
                if paren_cnt == 0: groups[-1] = ebnf_string[groups[-1]:(i + 1)]
            elif char == '(':
                paren_cnt += 1
        elif char == '"':
            match_string = True
            groups.append(i)
        elif char == '[':
            match_bracket = True
            groups.append(i)
        elif char == '(':
            paren_cnt = 1
            groups.append(i)
        elif re.match(r'[a-zA-Z0-9_]', char):
            if not match_nt:
                if match_nonterminals:
                    match_nt = True
                    groups.append(i)
                else:
                    groups.append(char)
        elif not char.isspace():
            groups.append(char)

    if match_nt: groups[-1] = ebnf_string[groups[-1]:]

    return groups


def _extract_rhs(ebnf_string: str, terminals: Dict[str, str]) -> str:
    if (
            len(ebnf_string) < 2 or
            (ebnf_string[0] == '[' and ebnf_string[-1] == ']') or
            ebnf_string[0] == ebnf_string[-1] == '"' or
            re.match(r'[a-zA-Z0-9_]+$', ebnf_string)
    ):
        return ebnf_string
    if not (ebnf_string[0] == '(' and ebnf_string[-1] == ')'):
        raise Exception

    groups = _extract_groups(ebnf_string, match_nonterminals=True)
    sub_elements = [_extract_rhs(g, terminals) for g in groups]
    out_list, i = [], 0

    while i < len(sub_elements):
        se_i = sub_elements[i]
        out_list.append(se_i)

        if se_i[0] == '[':
            i += 1

            while i < len(sub_elements) and len(se_next := sub_elements[i]) == 1 and se_next not in {'(', ')', '|'}:
                out_list[-1] += se_next
                i += 1

            i -= 1

        i += 1

    if all(x[0] in {'"', '['} for x in out_list):
        terminal_str = ' '.join(out_list)
        if terminal_str not in terminals.keys(): terminals.update({terminal_str: f'T{len(terminals)}'})

        return terminals[terminal_str]
    
    curr_regex = []
    out_list_compressed = []

    for i, x in enumerate(out_list):
        if x[0] in {'"', '['}:
            curr_regex.append(x)
        else:            
            if len(curr_regex) > 0:
                terminal_str = ' '.join(curr_regex)
                if terminal_str not in terminals.keys(): terminals.update({terminal_str: f'T{len(terminals)}'})
                out_list_compressed.append(terminals[terminal_str])
                curr_regex.clear()
            
            out_list_compressed.append(x)

    if len(curr_regex) > 0:
        terminal_str = ' '.join(curr_regex)
        if terminal_str not in terminals.keys(): terminals.update({terminal_str: f'T{len(terminals)}'})
        out_list_compressed.append(terminals[terminal_str])

    return f'({" ".join(out_list_compressed)})'


def remove_lparen(ebnf_string: str) -> str:
    out_str = ''

    for line in ebnf_string.split('\n'):
        if len(line.strip()) == 0:
            out_str += f'\n{line}'
        else:
            lhs, rhs = line.split(' ::= ', 1)
            paren_count, r_idx, esc_mode, rhs_new = 0, 0, False, ''

            for i, c in enumerate(rhs):
                if c == '\\':
                    esc_mode = not esc_mode
                elif esc_mode:
                    esc_mode = False
                elif c == '(':
                    if (paren_count := paren_count + 1) == 1: r_idx = i
                elif c == ')':
                    if (paren_count := paren_count - 1) == 0:
                        rhs_new = rhs[r_idx:(i + 1)]
                        break
            else:
                raise Exception

            out_str += f'\n{lhs} ::= {rhs_new}'

    return out_str


def ebnf_to_nf(ebnf_string: str) -> str:
    ebnf_lines = [x_str for x in ebnf_string.strip().split('\n') if len(x_str := x.strip()) > 0]
    productions = {line.split(' ::= ')[0].strip(): '' for line in ebnf_lines}
    terminals = {'""': '""'}

    for line in ebnf_lines:
        lhs, rhs = line.split(' ::= ', 1)
        lhs, rhs = lhs.strip(), rhs.strip()
        rhs = _extract_rhs(rhs, terminals)
        productions[lhs] = rhs

    terminals.pop('""')
    out_str = '\n'.join(f'{k} ::= {v.replace("(", "").replace(")", "")}' for k, v in productions.items())
    out_str += '\nTERMINALS:\n'
    out_str += '\n'.join(f'{v} ::= {ebnf_to_regex(k)}' for k, v in terminals.items())
    # out_str += '\n'.join(f'{v} ::= {ebnf_to_regex(k)}' for k, v in terminals.items())

    return out_str
