use ahash::RandomState as AHashRandomState;
use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use hf_hub::{api::sync::ApiBuilder, Repo, RepoType};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use regex::Regex;
use regex_automata::nfa::thompson::{State, NFA};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

type Symbol = String;
type Rhs = Vec<Symbol>;
type Grammar = BTreeMap<Symbol, BTreeSet<Rhs>>;
type Preterms = BTreeMap<Symbol, BTreeSet<u8>>;
type FastHashMap<K, V> = HashMap<K, V, AHashRandomState>;
type FastHashSet<T> = HashSet<T, AHashRandomState>;

fn fast_hash_map<K, V>() -> FastHashMap<K, V> {
    HashMap::with_hasher(AHashRandomState::new())
}

fn fast_hash_set<T>() -> FastHashSet<T> {
    HashSet::with_hasher(AHashRandomState::new())
}

#[derive(Parser, Debug)]
#[command(name = "cfgzip-preprocess")]
#[command(about = "Standalone Rust preprocessor for CFGzip grammars")]
struct Args {
    #[arg(long)]
    model_id: String,
    #[arg(long)]
    grammar_file: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "root")]
    start_symbol: String,
    #[arg(long)]
    n_logits: Option<usize>,
    #[arg(long, value_parser = parse_range)]
    ignore_range: Vec<(usize, usize)>,
    #[arg(long, default_value_t = false)]
    skip_null_bytes: bool,
    #[arg(long, default_value_t = false)]
    skip_repeat_bytes: bool,
    #[arg(long, default_value_t = 1)]
    num_threads: usize,
    #[arg(long, env = "HF_TOKEN")]
    hf_token: Option<String>,
    #[arg(long)]
    cache_dir: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    no_progress: bool,
}

fn parse_range(s: &str) -> Result<(usize, usize), String> {
    let (a, b) = s
        .split_once("..")
        .or_else(|| s.split_once(':'))
        .ok_or_else(|| "expected START..END".to_string())?;
    Ok((
        a.parse().map_err(|_| "bad start".to_string())?,
        b.parse().map_err(|_| "bad end".to_string())?,
    ))
}

fn progress_style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .unwrap()
        .progress_chars("=> ")
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.num_threads < 1 {
        bail!("--num-threads must be >= 1");
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.num_threads)
        .build_global()
        .ok();

    let grammar_str = fs::read_to_string(&args.grammar_file)
        .with_context(|| format!("reading {}", args.grammar_file.display()))?;
    let (tokenizer, eos_token_id) = load_tokenizer(&args)?;
    let tokens = tokenizer_tokens(&tokenizer)?;
    let n_logits = args
        .n_logits
        .unwrap_or_else(|| tokens.iter().map(|(_, id)| *id).max().unwrap_or(0) + 1);

    let (cfg, nfa_grammar, preterms, terminal_labels) =
        parse_cfg_str(&grammar_str, &args.start_symbol)?;
    let normed = normalize_cfg(cfg, nfa_grammar, terminal_labels)?;
    let eq = compute_token_classes(&normed, &preterms, &tokens, eos_token_id, n_logits, &args)?;
    write_output(&args.output, &eq, !args.no_progress)?;
    Ok(())
}

fn load_tokenizer(args: &Args) -> Result<(Tokenizer, usize)> {
    let mut builder = ApiBuilder::new();
    if let Some(cache) = &args.cache_dir {
        builder = builder.with_cache_dir(cache.clone());
    }
    if let Some(token) = &args.hf_token {
        builder = builder.with_token(Some(token.clone()));
    }
    let api = builder.build()?;
    let repo = api.repo(Repo::with_revision(
        args.model_id.clone(),
        RepoType::Model,
        "main".to_string(),
    ));
    let tokenizer_json = repo.get("tokenizer.json")?;
    let tokenizer = Tokenizer::from_file(&tokenizer_json)
        .map_err(|e| anyhow!("loading tokenizer.json: {e}"))?;

    let eos_token_id = discover_eos_token_id(&repo, &tokenizer)
        .ok_or_else(|| anyhow!("could not discover eos_token_id from tokenizer assets"))?;
    Ok((tokenizer, eos_token_id))
}

fn discover_eos_token_id(
    repo: &hf_hub::api::sync::ApiRepo,
    tokenizer: &Tokenizer,
) -> Option<usize> {
    for filename in [
        "tokenizer_config.json",
        "special_tokens_map.json",
        "config.json",
    ] {
        let Ok(path) = repo.get(filename) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        for key in ["eos_token_id", "eos_token"] {
            if let Some(value) = json.get(key) {
                if let Some(id) = eos_from_value(value, tokenizer) {
                    return Some(id);
                }
            }
        }
    }
    tokenizer.token_to_id("<|endoftext|>").map(|x| x as usize)
}

fn eos_from_value(v: &Value, tokenizer: &Tokenizer) -> Option<usize> {
    match v {
        Value::Number(n) => n.as_u64().map(|x| x as usize),
        Value::String(s) => tokenizer.token_to_id(s).map(|x| x as usize),
        Value::Object(m) => m
            .get("id")
            .and_then(|x| x.as_u64().map(|y| y as usize))
            .or_else(|| {
                m.get("content")
                    .and_then(|x| x.as_str())
                    .and_then(|s| tokenizer.token_to_id(s).map(|x| x as usize))
            }),
        _ => None,
    }
}

fn gpt2_byte_decoder() -> HashMap<char, u8> {
    let mut out = HashMap::new();
    let mut n = 0u32;
    for b in 0u32..=255 {
        if b < 33 || (127..=160).contains(&b) || b == 173 {
            out.insert(char::from_u32(n + 256).unwrap(), b as u8);
            n += 1;
        } else {
            out.insert(char::from_u32(b).unwrap(), b as u8);
        }
    }
    out
}

fn token_to_bytes(token: &str, byte_decoder: &HashMap<char, u8>) -> Result<Vec<u8>> {
    let re = Regex::new(r"^<0x([0-9A-Fa-f]{2})>$").unwrap();
    if let Some(caps) = re.captures(token) {
        return Ok(vec![u8::from_str_radix(&caps[1], 16)?]);
    }
    token
        .chars()
        .map(|c| {
            byte_decoder.get(&c).copied().ok_or_else(|| {
                anyhow!("token contains character not in GPT-2 byte decoder: {c:?} in {token:?}")
            })
        })
        .collect()
}

fn tokenizer_tokens(tokenizer: &Tokenizer) -> Result<Vec<(Vec<u8>, usize)>> {
    let dec = gpt2_byte_decoder();
    let vocab = tokenizer.get_vocab(true);
    let mut out = Vec::with_capacity(vocab.len());
    for (_tok, id) in vocab {
        let tok = tokenizer
            .id_to_token(id)
            .ok_or_else(|| anyhow!("missing token string for id {id}"))?;
        out.push((token_to_bytes(&tok, &dec)?, id as usize));
    }
    Ok(out)
}

#[derive(Clone, Debug)]
enum Tok {
    Str(String, String),
    Cls(String, String),
    Group(String, String),
    Nt(String, String),
    Alt,
}

fn strip_comment(line: &str) -> String {
    let (mut in_str, mut in_cls, mut esc) = (false, false, false);
    for (i, c) in line.char_indices() {
        if esc {
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if in_str {
            if c == '"' {
                in_str = false;
            }
        } else if in_cls {
            if c == ']' {
                in_cls = false;
            }
        } else if c == '"' {
            in_str = true;
        } else if c == '[' {
            in_cls = true;
        } else if c == '#' {
            return line[..i].to_string();
        }
    }
    line.to_string()
}

fn join_lines(lines: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut parts = Vec::new();
    let (mut depth, mut in_str, mut in_cls, mut esc) = (0i32, false, false, false);
    for line in lines {
        parts.push(line.clone());
        for c in line.chars() {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if in_str {
                if c == '"' {
                    in_str = false;
                }
            } else if in_cls {
                if c == ']' {
                    in_cls = false;
                }
            } else if c == '"' {
                in_str = true;
            } else if c == '[' {
                in_cls = true;
            } else if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            }
        }
        if depth <= 0 {
            result.push(parts.join(" "));
            parts.clear();
            depth = 0;
        }
    }
    if !parts.is_empty() {
        result.push(parts.join(" "));
    }
    result
}

fn read_quant(chars: &[char], j: usize) -> (String, usize) {
    if j < chars.len() && matches!(chars[j], '*' | '+' | '?') {
        (chars[j].to_string(), j + 1)
    } else {
        (String::new(), j)
    }
}

fn tokenize(rhs: &str) -> Vec<Tok> {
    let chars: Vec<char> = rhs.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' {
            break;
        } else if c == '|' {
            out.push(Tok::Alt);
            i += 1;
        } else if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                } else if chars[i] == '"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            let (q, j) = read_quant(&chars, i);
            out.push(Tok::Str(chars[start..i].iter().collect(), q));
            i = j;
        } else if c == '[' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                } else if chars[i] == ']' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            let (q, j) = read_quant(&chars, i);
            out.push(Tok::Cls(chars[start..i].iter().collect(), q));
            i = j;
        } else if c == '(' {
            let start = i;
            let (mut depth, mut esc, mut in_str, mut in_cls) = (1i32, false, false, false);
            i += 1;
            while i < chars.len() && depth > 0 {
                let ch = chars[i];
                if esc {
                    esc = false;
                } else if ch == '\\' {
                    esc = true;
                } else if in_str {
                    if ch == '"' {
                        in_str = false;
                    }
                } else if in_cls {
                    if ch == ']' {
                        in_cls = false;
                    }
                } else if ch == '"' {
                    in_str = true;
                } else if ch == '[' {
                    in_cls = true;
                } else if ch == '(' {
                    depth += 1;
                } else if ch == ')' {
                    depth -= 1;
                }
                i += 1;
            }
            let (q, j) = read_quant(&chars, i);
            out.push(Tok::Group(chars[start..i].iter().collect(), q));
            i = j;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
            {
                i += 1;
            }
            let (q, j) = read_quant(&chars, i);
            out.push(Tok::Nt(chars[start..i].iter().collect(), q));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn tok_is_terminal(tok: &Tok) -> bool {
    match tok {
        Tok::Str(_, _) | Tok::Cls(_, _) => true,
        Tok::Group(text, _) => tokenize(&text[1..text.len() - 1])
            .iter()
            .filter(|t| !matches!(t, Tok::Alt))
            .all(tok_is_terminal),
        _ => false,
    }
}

fn escape_literal(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            if matches!(n, 'n' | 't' | 'r' | 'f' | 'v' | '\\' | '"' | '\'' | '/') {
                out.push('\\');
                out.push(n);
                i += 2;
            } else if n == 'x' && i + 3 < chars.len() {
                out.extend(chars[i..i + 4].iter());
                i += 4;
            } else if n == 'u' && i + 5 < chars.len() {
                out.extend(chars[i..i + 6].iter());
                i += 6;
            } else if n == 'U' && i + 9 < chars.len() {
                out.extend(chars[i..i + 10].iter());
                i += 10;
            } else {
                out.push_str(&regex::escape(&chars[i].to_string()));
                i += 1;
            }
        } else {
            out.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }
    out
}

fn wrap_for_quant(s: &str) -> String {
    if s.chars().count() == 1 || (s.starts_with('[') && s.ends_with(']')) {
        s.to_string()
    } else if s.starts_with('(') && s.ends_with(')') {
        format!("(?:{})", &s[1..s.len() - 1])
    } else {
        format!("(?:{s})")
    }
}

fn extract_groups(group: &str) -> Vec<String> {
    let inner: Vec<char> = group[1..group.len() - 1].chars().collect();
    let mut out = Vec::new();
    let (mut i, mut esc) = (0, false);
    while i < inner.len() {
        let c = inner[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '[' || c == '(' {
            let start = i;
            if c == '"' {
                i += 1;
                while i < inner.len() {
                    if inner[i] == '\\' {
                        i += 2;
                    } else if inner[i] == '"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            } else if c == '[' {
                i += 1;
                while i < inner.len() {
                    if inner[i] == '\\' {
                        i += 2;
                    } else if inner[i] == ']' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            } else {
                let mut depth = 1i32;
                i += 1;
                while i < inner.len() && depth > 0 {
                    if esc {
                        esc = false;
                    } else if inner[i] == '\\' {
                        esc = true;
                    } else if inner[i] == '(' {
                        depth += 1;
                    } else if inner[i] == ')' {
                        depth -= 1;
                    }
                    i += 1;
                }
            }
            out.push(inner[start..i].iter().collect());
        } else if c.is_alphanumeric() {
            out.push(c.to_string());
            i += 1;
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
}

fn gbnf_to_regex_rec(s: &str) -> Result<String> {
    let s = s.trim();
    if s.len() < 2 || (s.starts_with('[') && s.ends_with(']')) {
        return Ok(s.replace("\\\"", "\"").replace("\\'", "'"));
    }
    if s.starts_with('"') && s.ends_with('"') {
        return Ok(escape_literal(&s[1..s.len() - 1]));
    }
    if s.starts_with('(') && s.ends_with(')') {
        let groups = extract_groups(s);
        let mut parts = Vec::new();
        let mut i = 0;
        while i < groups.len() {
            let converted = gbnf_to_regex_rec(&groups[i])?;
            if i + 1 < groups.len() && matches!(groups[i + 1].as_str(), "*" | "+" | "?") {
                parts.push(format!("{}{}", wrap_for_quant(&converted), groups[i + 1]));
                i += 2;
            } else {
                parts.push(converted);
                i += 1;
            }
        }
        return Ok(format!("({})", parts.join("")));
    }
    bail!("unrecognized gbnf atom: {s:?}")
}

fn gbnf_to_regex(s: &str) -> Result<String> {
    let wrapped = gbnf_to_regex_rec(&format!("({s})"))?;
    Ok(wrapped[1..wrapped.len() - 1].to_string())
}

fn sdict_add<K: Ord, V: Ord>(m: &mut BTreeMap<K, BTreeSet<V>>, k: K, v: V) {
    m.entry(k).or_default().insert(v);
}

fn add_terminal(terminals: &mut BTreeMap<String, String>, pattern: String) -> String {
    if let Some(lbl) = terminals.get(&pattern) {
        return lbl.clone();
    }
    let lbl = format!("__T{}", terminals.len() - 1);
    terminals.insert(pattern, lbl.clone());
    lbl
}

fn make_quant_aux(base: &str, quant: &str, prods: &mut Grammar, aux: &mut usize) -> String {
    let name = format!("__rep{}", *aux);
    *aux += 1;
    match quant {
        "*" => {
            sdict_add(prods, name.clone(), vec![]);
            sdict_add(prods, name.clone(), vec![base.to_string(), name.clone()]);
        }
        "+" => {
            sdict_add(prods, name.clone(), vec![base.to_string()]);
            sdict_add(prods, name.clone(), vec![base.to_string(), name.clone()]);
        }
        "?" => {
            sdict_add(prods, name.clone(), vec![]);
            sdict_add(prods, name.clone(), vec![base.to_string()]);
        }
        _ => {}
    }
    name
}

fn process_alt(
    tokens: &[Tok],
    terminals: &mut BTreeMap<String, String>,
    prods: &mut Grammar,
    aux: &mut usize,
) -> Rhs {
    let mut syms = Vec::new();
    let mut term_run: Vec<Tok> = Vec::new();
    let flush =
        |syms: &mut Vec<String>, run: &mut Vec<Tok>, terminals: &mut BTreeMap<String, String>| {
            if run.is_empty() {
                return;
            }
            let pieces: Vec<String> = run
                .iter()
                .map(|t| match t {
                    Tok::Str(x, q) | Tok::Cls(x, q) | Tok::Group(x, q) => format!("{x}{q}"),
                    _ => String::new(),
                })
                .collect();
            syms.push(add_terminal(terminals, pieces.join(" ")));
            run.clear();
        };
    for tok in tokens {
        if tok_is_terminal(tok) {
            term_run.push(tok.clone());
            continue;
        }
        match tok {
            Tok::Nt(name, q) => {
                flush(&mut syms, &mut term_run, terminals);
                syms.push(if q.is_empty() {
                    name.clone()
                } else {
                    make_quant_aux(name, q, prods, aux)
                });
            }
            Tok::Group(text, q) => {
                flush(&mut syms, &mut term_run, terminals);
                let g = format!("__grp{}", *aux);
                *aux += 1;
                parse_rhs(&g, &text[1..text.len() - 1], terminals, prods, aux);
                let sym = if q.is_empty() {
                    g
                } else {
                    make_quant_aux(&g, q, prods, aux)
                };
                syms.push(sym);
            }
            _ => {}
        }
    }
    flush(&mut syms, &mut term_run, terminals);
    syms
}

fn parse_rhs(
    lhs: &str,
    rhs: &str,
    terminals: &mut BTreeMap<String, String>,
    prods: &mut Grammar,
    aux: &mut usize,
) {
    let tokens = tokenize(rhs);
    let mut alts: Vec<Vec<Tok>> = vec![Vec::new()];
    for tok in tokens {
        if matches!(tok, Tok::Alt) {
            alts.push(Vec::new());
        } else {
            alts.last_mut().unwrap().push(tok);
        }
    }
    prods.entry(lhs.to_string()).or_default();
    for alt in alts {
        if alt.is_empty() {
            sdict_add(prods, lhs.to_string(), vec![]);
        } else {
            let rhs = process_alt(&alt, terminals, prods, aux);
            sdict_add(prods, lhs.to_string(), rhs);
        }
    }
}

fn parse_gbnf(gbnf: &str) -> Result<(BTreeMap<String, String>, Grammar)> {
    let raw: Vec<String> = gbnf
        .lines()
        .filter_map(|line| {
            let s = strip_comment(line).trim().to_string();
            (!s.is_empty()).then_some(s)
        })
        .collect();
    let lines = join_lines(raw);
    let mut parsed = Vec::new();
    for line in lines {
        let (lhs, rhs) = line
            .split_once(" ::= ")
            .ok_or_else(|| anyhow!("invalid GBNF rule: {line:?}"))?;
        parsed.push((lhs.trim().to_string(), rhs.trim().to_string()));
    }
    let mut terminals = BTreeMap::from([("\"\"".to_string(), "\"\"".to_string())]);
    let mut prods = Grammar::new();
    let mut aux = 0usize;
    for (lhs, _) in &parsed {
        prods.entry(lhs.clone()).or_default();
    }
    for (lhs, rhs) in parsed {
        parse_rhs(&lhs, &rhs, &mut terminals, &mut prods, &mut aux);
    }
    terminals.remove("\"\"");
    let mut flipped = BTreeMap::new();
    for (pat, lbl) in terminals {
        flipped.insert(lbl, gbnf_to_regex(&pat)?);
    }
    Ok((flipped, prods))
}

fn parse_cfg_str(
    cfg_str: &str,
    start_symbol: &str,
) -> Result<(Grammar, Grammar, Preterms, BTreeMap<String, String>)> {
    let (terminals, mut cfg_dict) = parse_gbnf(cfg_str)?;
    if !cfg_dict.contains_key(start_symbol) {
        bail!(
            "start symbol {start_symbol:?} not found; available non-terminals: {:?}",
            cfg_dict.keys().collect::<Vec<_>>()
        );
    }
    let start = cfg_dict.remove(start_symbol).unwrap();
    cfg_dict.insert("S".to_string(), start);
    let (mut nfa_grammar, mut preterms, mut cfg_out, mut terminal_labels) = (
        Grammar::new(),
        Preterms::new(),
        Grammar::new(),
        BTreeMap::new(),
    );
    for (label, regex) in terminals {
        sdict_add(&mut cfg_out, format!("PT[{label}]"), vec![label.clone()]);
        terminal_labels.insert(format!("PT[{label}]"), label.clone());
        let (g, p) = regex_to_gnf_cfg(&label, &regex)?;
        nfa_grammar.extend(g);
        preterms.extend(p);
    }
    for (a, prods) in cfg_dict {
        for beta in prods {
            let rhs = beta
                .into_iter()
                .map(|x| {
                    if terminal_labels.contains_key(&format!("PT[{x}]")) {
                        format!("PT[{x}]")
                    } else if x == "\"\"" {
                        "ε".to_string()
                    } else {
                        x
                    }
                })
                .collect();
            sdict_add(&mut cfg_out, a.clone(), rhs);
        }
    }
    Ok((cfg_out, nfa_grammar, preterms, terminal_labels))
}

fn epsilon_closure(nfa: &NFA, sid: usize) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![sid];
    while let Some(s) = stack.pop() {
        if !seen.insert(s) {
            continue;
        }
        match nfa.state(regex_automata::util::primitives::StateID::new(s).unwrap()) {
            State::Union { alternates } => {
                stack.extend(alternates.iter().map(|x| x.as_usize()));
            }
            State::BinaryUnion { alt1, alt2 } => {
                stack.push(alt1.as_usize());
                stack.push(alt2.as_usize());
            }
            State::Capture { next, .. } | State::Look { next, .. } => stack.push(next.as_usize()),
            _ => {}
        }
    }
    seen
}

fn regex_to_gnf_cfg(label: &str, regex: &str) -> Result<(Grammar, Preterms)> {
    let nfa = NFA::compiler()
        .configure(NFA::config().which_captures(regex_automata::nfa::thompson::WhichCaptures::None))
        .build(regex)
        .with_context(|| format!("compiling terminal regex {regex:?}"))?;
    let start = nfa.start_anchored().as_usize();
    let reachable = reachable_from_start(&nfa, start);
    let mut cfg = Grammar::new();
    let mut preterms = Preterms::new();
    let mut p_count = 0usize;

    for sid in reachable.iter().copied() {
        let src_sym = if sid == start {
            label.to_string()
        } else {
            format!("{label}[q{sid}]")
        };
        let closure = epsilon_closure(&nfa, sid);
        for csid in closure {
            let state = nfa.state(regex_automata::util::primitives::StateID::new(csid).unwrap());
            let mut transitions = Vec::new();
            match state {
                State::ByteRange { trans } => transitions.push(*trans),
                State::Sparse(s) => transitions.extend(s.transitions.iter().copied()),
                State::Dense(d) => {
                    for b in 0u8..=255 {
                        if let Some(next) = d.matches_byte(b) {
                            transitions.push(regex_automata::nfa::thompson::Transition {
                                start: b,
                                end: b,
                                next,
                            });
                        }
                    }
                }
                _ => {}
            }
            for trans in transitions {
                let next = trans.next.as_usize();
                let next_closure = epsilon_closure(&nfa, next);
                let accepting = next_closure.iter().any(|&x| {
                    matches!(
                        nfa.state(regex_automata::util::primitives::StateID::new(x).unwrap()),
                        State::Match { .. }
                    )
                });
                let has_out = next_closure.iter().any(|&x| {
                    matches!(
                        nfa.state(regex_automata::util::primitives::StateID::new(x).unwrap()),
                        State::ByteRange { .. } | State::Sparse(_) | State::Dense(_)
                    )
                });
                let p = format!("{label}[P{p_count}]");
                p_count += 1;
                let bytes = (trans.start..=trans.end).collect::<BTreeSet<_>>();
                preterms.insert(p.clone(), bytes);
                if accepting {
                    sdict_add(&mut cfg, src_sym.clone(), vec![p.clone()]);
                }
                if !accepting || has_out {
                    let dst = if next == start {
                        label.to_string()
                    } else {
                        format!("{label}[q{next}]")
                    };
                    sdict_add(&mut cfg, src_sym.clone(), vec![p, dst]);
                }
            }
        }
    }
    Ok((cfg, preterms))
}

fn reachable_from_start(nfa: &NFA, start: usize) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(sid) = stack.pop() {
        if !seen.insert(sid) {
            continue;
        }
        let state = nfa.state(regex_automata::util::primitives::StateID::new(sid).unwrap());
        match state {
            State::Union { alternates } => stack.extend(alternates.iter().map(|x| x.as_usize())),
            State::BinaryUnion { alt1, alt2 } => {
                stack.push(alt1.as_usize());
                stack.push(alt2.as_usize());
            }
            State::Capture { next, .. } | State::Look { next, .. } => stack.push(next.as_usize()),
            State::ByteRange { trans } => stack.push(trans.next.as_usize()),
            State::Sparse(s) => stack.extend(s.transitions.iter().map(|t| t.next.as_usize())),
            State::Dense(d) => {
                for b in 0u8..=255 {
                    if let Some(next) = d.matches_byte(b) {
                        stack.push(next.as_usize());
                    }
                }
            }
            State::Fail | State::Match { .. } => {}
        }
    }
    seen
}

fn remove_epsilon_productions(cfg: &mut Grammar) {
    let mut empty = BTreeSet::from(["ε".to_string()]);
    let mut e_prev = BTreeSet::new();
    let mut cfg_prev = cfg.clone();
    let mut poss_empty = BTreeSet::new();
    while e_prev != empty {
        e_prev = empty.clone();
        let mut cfg_next = Grammar::new();
        for lhs in cfg_prev.keys().cloned().collect::<Vec<_>>() {
            if empty.contains(&lhs) {
                continue;
            }
            let mut lhs_empty = true;
            for rhs in cfg_prev.get(&lhs).cloned().unwrap_or_default() {
                let rhs_new: Rhs = rhs.into_iter().filter(|x| !empty.contains(x)).collect();
                if rhs_new.is_empty() {
                    poss_empty.insert(lhs.clone());
                } else {
                    sdict_add(&mut cfg_next, lhs.clone(), rhs_new);
                    lhs_empty = false;
                }
            }
            if lhs_empty {
                empty.insert(lhs);
            }
        }
        cfg_prev = cfg_next;
    }
    poss_empty = poss_empty.difference(&empty).cloned().collect();
    e_prev.clear();
    while e_prev != poss_empty {
        e_prev = poss_empty.clone();
        for (lhs, prods) in cfg_prev.clone() {
            if prods
                .iter()
                .any(|rhs| rhs.iter().all(|x| poss_empty.contains(x)))
            {
                poss_empty.insert(lhs);
            }
        }
    }
    fn variants(
        rhs: &[String],
        poss: &BTreeSet<String>,
        i: usize,
        out: &mut Vec<String>,
        acc: &mut BTreeSet<Rhs>,
    ) {
        if i == rhs.len() {
            if !out.is_empty() {
                acc.insert(out.clone());
            }
            return;
        }
        out.push(rhs[i].clone());
        variants(rhs, poss, i + 1, out, acc);
        out.pop();
        if poss.contains(&rhs[i]) {
            variants(rhs, poss, i + 1, out, acc);
        }
    }
    let mut cfg_next = Grammar::new();
    for (lhs, prods) in cfg_prev {
        for rhs in prods {
            let mut acc = BTreeSet::new();
            variants(&rhs, &poss_empty, 0, &mut Vec::new(), &mut acc);
            for r in acc {
                sdict_add(&mut cfg_next, lhs.clone(), r);
            }
        }
    }
    *cfg = cfg_next;
}

fn remove_unary_rules(
    cfg: &mut Grammar,
    terminals: impl IntoIterator<Item = String>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut terminal_map: BTreeMap<String, BTreeSet<String>> = terminals
        .into_iter()
        .map(|t| (t.clone(), BTreeSet::from([t])))
        .collect();
    loop {
        let mut to_remove = None;
        for (lhs, prods) in cfg.iter() {
            for rhs in prods {
                if rhs.len() == 1 && rhs[0] != *lhs {
                    to_remove = Some((lhs.clone(), rhs[0].clone()));
                    break;
                }
            }
            if to_remove.is_some() {
                break;
            }
        }
        let Some((x, y)) = to_remove else { break };
        if let Some(prods) = cfg.get_mut(&x) {
            prods.remove(&vec![y.clone()]);
        }
        if let Some(y_prods) = cfg.get(&y).cloned() {
            for rhs in y_prods {
                sdict_add(cfg, x.clone(), rhs);
            }
        }
        if let Some(terms) = terminal_map.get(&y).cloned() {
            terminal_map.entry(x).or_default().extend(terms);
        }
        let keys: Vec<_> = cfg.keys().cloned().collect();
        for k in keys {
            if let Some(prods) = cfg.get_mut(&k) {
                prods.remove(&vec![k.clone()]);
                if prods.is_empty() {
                    cfg.remove(&k);
                }
            }
        }
    }
    let mut used = BTreeSet::from(["S".to_string()]);
    for prods in cfg.values() {
        for rhs in prods {
            used.extend(rhs.iter().cloned());
        }
    }
    terminal_map.retain(|k, _| used.contains(k));
    terminal_map
}

fn remove_unreachable_symbols(cfg: &mut Grammar) {
    let mut reachable = BTreeSet::from(["S".to_string()]);
    let mut prev = BTreeSet::new();
    while prev != reachable {
        let update: Vec<_> = reachable.difference(&prev).cloned().collect();
        prev = reachable.clone();
        for r in update {
            if let Some(prods) = cfg.get(&r) {
                for rhs in prods {
                    reachable.extend(rhs.iter().cloned());
                }
            }
        }
    }
    cfg.retain(|k, _| reachable.contains(k));
}

fn cfg_to_gnf(mut cfg: Grammar, preterminals: BTreeSet<String>) -> Grammar {
    let keys: Vec<_> = cfg.keys().cloned().collect();
    for k in keys {
        let mut changed = true;
        while changed {
            changed = false;
            let prods = cfg.get(&k).cloned().unwrap_or_default();
            let mut newp = BTreeSet::new();
            for beta in prods.iter() {
                if beta.first() == Some(&k) {
                    let lr = format!("LR[{k}]");
                    sdict_add(&mut cfg, lr.clone(), beta[1..].to_vec());
                    let mut rec = beta[1..].to_vec();
                    rec.push(lr.clone());
                    sdict_add(&mut cfg, lr.clone(), rec);
                    for prod in &prods {
                        if prod.first() != Some(&k) && prod.last() != Some(&lr) {
                            let mut p = prod.clone();
                            p.push(lr.clone());
                            newp.insert(p);
                        }
                    }
                    changed = true;
                } else {
                    newp.insert(beta.clone());
                }
            }
            if changed {
                cfg.insert(k.clone(), newp);
            }
        }
    }
    let mut cfg_keys = vec!["S".to_string()];
    let mut prev_level = vec!["S".to_string()];
    let mut visited = BTreeSet::from(["S".to_string()]);
    while !prev_level.is_empty() {
        let mut next = Vec::new();
        for x in prev_level {
            if let Some(prods) = cfg.get(&x) {
                for beta in prods {
                    for b in beta {
                        if cfg.contains_key(b) && visited.insert(b.clone()) {
                            next.push(b.clone());
                        }
                    }
                }
            }
        }
        next.sort_by_key(|z| {
            std::cmp::Reverse(
                cfg.get(z)
                    .map(|ps| ps.iter().map(|w| w.len()).sum::<usize>())
                    .unwrap_or(0),
            )
        });
        cfg_keys.extend(next.clone());
        prev_level = next;
    }
    let ord: HashMap<_, _> = cfg_keys
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, k)| (k, i))
        .collect();
    let keys_before: BTreeSet<_> = cfg.keys().cloned().collect();
    for k in cfg_keys.iter().skip(2).cloned().collect::<Vec<_>>() {
        loop {
            let prods = cfg.get(&k).cloned().unwrap_or_default();
            let mut changed = false;
            let mut newp = BTreeSet::new();
            for beta in prods.iter() {
                let Some(beta0) = beta.first() else { continue };
                if preterminals.contains(beta0)
                    || ord.get(beta0).copied().unwrap_or(usize::MAX) > ord[&k]
                {
                    newp.insert(beta.clone());
                } else if beta0 == &k {
                    let lr = format!("LR[{k}]");
                    sdict_add(&mut cfg, lr.clone(), beta[1..].to_vec());
                    let mut rec = beta[1..].to_vec();
                    rec.push(lr.clone());
                    sdict_add(&mut cfg, lr.clone(), rec);
                    for prod in &prods {
                        if prod.first() != Some(&k) && prod.last() != Some(&lr) {
                            let mut p = prod.clone();
                            p.push(lr.clone());
                            newp.insert(p);
                        }
                    }
                    changed = true;
                } else if let Some(subs) = cfg.get(beta0).cloned() {
                    for prod in subs {
                        let mut p = prod;
                        p.extend(beta[1..].iter().cloned());
                        newp.insert(p);
                    }
                    changed = true;
                } else {
                    newp.insert(beta.clone());
                }
            }
            if !changed {
                break;
            }
            cfg.insert(k.clone(), newp);
        }
    }
    cfg_keys.extend(
        cfg.keys()
            .filter(|k| !keys_before.contains(*k))
            .cloned()
            .collect::<Vec<_>>(),
    );
    for k in cfg_keys {
        loop {
            let prods = cfg.get(&k).cloned().unwrap_or_default();
            let mut changed = false;
            let mut newp = BTreeSet::new();
            for beta in prods {
                let Some(beta0) = beta.first() else { continue };
                if preterminals.contains(beta0) {
                    newp.insert(beta);
                } else if let Some(subs) = cfg.get(beta0).cloned() {
                    for prod in subs {
                        let mut p = prod;
                        p.extend(beta[1..].iter().cloned());
                        newp.insert(p);
                    }
                    changed = true;
                } else {
                    newp.insert(beta);
                }
            }
            cfg.insert(k.clone(), newp);
            if !changed {
                break;
            }
        }
    }
    remove_unreachable_symbols(&mut cfg);
    loop {
        let keys: Vec<_> = cfg.keys().filter(|&k| k != "S").cloned().collect();
        let mut merged = false;
        'outer: for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                if cfg.get(&keys[i]) == cfg.get(&keys[j]) {
                    let keep = keys[i].clone();
                    let drop = keys[j].clone();
                    for prods in cfg.values_mut() {
                        *prods = prods
                            .iter()
                            .map(|rhs| {
                                rhs.iter()
                                    .map(|b| if b == &drop { keep.clone() } else { b.clone() })
                                    .collect()
                            })
                            .collect();
                    }
                    cfg.remove(&drop);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            break;
        }
    }
    cfg
}

fn normalize_cfg(
    mut cfg: Grammar,
    nfa_grammar: Grammar,
    terminal_labels: BTreeMap<String, String>,
) -> Result<Grammar> {
    remove_epsilon_productions(&mut cfg);
    let terminal_map = remove_unary_rules(&mut cfg, terminal_labels.keys().cloned());
    remove_unreachable_symbols(&mut cfg);
    for (x, ys) in terminal_map {
        for y in ys {
            if let Some(lbl) = terminal_labels.get(&y) {
                sdict_add(&mut cfg, x.clone(), vec![lbl.clone()]);
            }
        }
    }
    let terminal_values = terminal_labels.values().cloned().collect();
    let cfg = cfg_to_gnf(cfg, terminal_values);
    let mut out = Grammar::new();
    for (a, prods) in cfg {
        if terminal_labels.contains_key(&a) {
            continue;
        }
        for beta in prods {
            if beta.is_empty() {
                continue;
            }
            let beta1: Vec<_> = beta[1..]
                .iter()
                .map(|x| terminal_labels.get(x).cloned().unwrap_or_else(|| x.clone()))
                .collect();
            if let Some(nfa_prods) = nfa_grammar.get(&beta[0]) {
                for nfa_prod in nfa_prods {
                    let mut rhs = nfa_prod.clone();
                    rhs.extend(beta1.iter().cloned());
                    sdict_add(&mut out, a.clone(), rhs);
                }
            }
        }
    }
    let all_symbols: BTreeSet<_> = out
        .values()
        .flat_map(|ps| ps.iter().flat_map(|rhs| rhs.iter().cloned()))
        .collect();
    for (k, v) in nfa_grammar {
        if k.contains("[q") || all_symbols.contains(&k) {
            out.insert(k, v);
        }
    }
    Ok(out)
}

#[derive(Default)]
struct EquivOut {
    token_classes: Vec<i32>,
    invalid_tokens: Vec<i32>,
    class_representatives: Vec<Vec<u8>>,
}

#[derive(Clone)]
struct IntGrammar {
    preterms_rev: [Vec<usize>; 256],
    nt_map: Vec<Vec<usize>>,
    transitions: Vec<FastHashMap<usize, Vec<Vec<usize>>>>,
    stack_adj: Vec<FastHashSet<Option<usize>>>,
    start: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SearchState {
    in_stack: usize,
    out_stack: usize,
    prev_symbol: Option<usize>,
    allow_bt: bool,
}

#[derive(Clone)]
struct StackInterner {
    stacks: Vec<Vec<usize>>,
    ids: FastHashMap<Vec<usize>, usize>,
}

#[derive(Default)]
struct TokenTrieNode {
    children: BTreeMap<u8, usize>,
    token_ids: Vec<usize>,
}

struct TrieJob {
    node_id: usize,
    frontier: FastHashSet<SearchState>,
    stacks: StackInterner,
}

struct LocalTokenResult {
    token_id: usize,
    stack_pairs: Option<Vec<(usize, usize)>>,
}

struct TokenStackResult {
    token_id: usize,
    stack_set: usize,
    stack_pairs: Option<Vec<(usize, usize)>>,
}

struct TokenTraversalResults {
    stack_sets: Vec<StackInterner>,
    tokens: Vec<TokenStackResult>,
}

impl StackInterner {
    fn new() -> Self {
        let mut ids = fast_hash_map();
        ids.insert(Vec::new(), 0);
        Self {
            stacks: vec![Vec::new()],
            ids,
        }
    }

    fn intern_vec(&mut self, stack: Vec<usize>) -> usize {
        if let Some(&id) = self.ids.get(&stack) {
            return id;
        }
        let id = self.stacks.len();
        self.stacks.push(stack.clone());
        self.ids.insert(stack, id);
        id
    }

    fn intern_slice(&mut self, stack: &[usize]) -> usize {
        if stack.is_empty() {
            return 0;
        }
        self.intern_vec(stack.to_vec())
    }

    fn push(&mut self, stack_id: usize, sym: usize) -> usize {
        let mut stack = self.stacks[stack_id].clone();
        stack.push(sym);
        self.intern_vec(stack)
    }

    fn prepend_to_tail(&mut self, prefix: &[usize], tail_id: usize, tail_start: usize) -> usize {
        if prefix.is_empty() && tail_start == self.stacks[tail_id].len() {
            return 0;
        }
        let tail = &self.stacks[tail_id];
        let mut stack = Vec::with_capacity(prefix.len() + tail.len().saturating_sub(tail_start));
        stack.extend_from_slice(prefix);
        stack.extend_from_slice(&tail[tail_start..]);
        self.intern_vec(stack)
    }

    fn first(&self, stack_id: usize) -> Option<usize> {
        self.stacks[stack_id].first().copied()
    }

    fn is_empty(&self, stack_id: usize) -> bool {
        self.stacks[stack_id].is_empty()
    }
}

fn rebase_frontier(
    frontier: &FastHashSet<SearchState>,
    stacks: &StackInterner,
) -> (StackInterner, FastHashSet<SearchState>) {
    let mut rebased_stacks = StackInterner::new();
    let rebased_frontier = frontier
        .iter()
        .map(|state| SearchState {
            in_stack: rebased_stacks.intern_slice(&stacks.stacks[state.in_stack]),
            out_stack: rebased_stacks.intern_slice(&stacks.stacks[state.out_stack]),
            prev_symbol: state.prev_symbol,
            allow_bt: state.allow_bt,
        })
        .collect();
    (rebased_stacks, rebased_frontier)
}

fn intern_global_stack(
    global_stacks: &mut FastHashMap<Vec<usize>, usize>,
    stack: &[usize],
) -> usize {
    if let Some(&id) = global_stacks.get(stack) {
        return id;
    }
    let id = global_stacks.len();
    global_stacks.insert(stack.to_vec(), id);
    id
}

fn compute_stack_adj(
    grammar: &HashMap<usize, Vec<Vec<usize>>>,
    start: usize,
) -> FastHashMap<usize, FastHashSet<Option<usize>>> {
    let mut all = grammar.keys().copied().collect::<FastHashSet<_>>();
    let mut neigh: FastHashMap<usize, FastHashSet<usize>> = fast_hash_map();
    for prods in grammar.values() {
        for beta in prods {
            all.extend(beta.iter().skip(1).copied());
            for pair in beta.windows(2).skip(1) {
                neigh.entry(pair[1]).or_default().insert(pair[0]);
            }
        }
    }
    all.remove(&start);
    fn rec_adj(
        x: usize,
        grammar: &HashMap<usize, Vec<Vec<usize>>>,
        visited: &mut FastHashSet<usize>,
        out: &mut FastHashSet<usize>,
    ) {
        visited.insert(x);
        if let Some(prods) = grammar.get(&x) {
            for prod in prods {
                if prod.len() == 1 {
                    out.insert(x);
                } else if let Some(&n) = prod.last() {
                    if !visited.contains(&n) {
                        rec_adj(n, grammar, visited, out);
                    }
                }
            }
        }
    }
    let mut adj = fast_hash_map();
    for a in all {
        let mut s = [None].into_iter().collect::<FastHashSet<_>>();
        if let Some(bs) = neigh.get(&a) {
            for &b in bs {
                let mut out = fast_hash_set();
                rec_adj(b, grammar, &mut fast_hash_set(), &mut out);
                s.extend(out.into_iter().map(Some));
            }
        }
        adj.insert(a, s);
    }
    adj.insert(start, fast_hash_set());
    adj
}

fn build_token_trie(tasks: &[(Vec<u8>, usize)]) -> Vec<TokenTrieNode> {
    let mut nodes = vec![TokenTrieNode::default()];
    for (tok, id) in tasks {
        let mut node = 0usize;
        for &b in tok {
            if let Some(&next) = nodes[node].children.get(&b) {
                node = next;
            } else {
                let next = nodes.len();
                nodes.push(TokenTrieNode::default());
                nodes[node].children.insert(b, next);
                node = next;
            }
        }
        nodes[node].token_ids.push(*id);
    }
    nodes
}

fn compute_subtree_token_counts(nodes: &[TokenTrieNode]) -> Vec<usize> {
    fn rec(nodes: &[TokenTrieNode], node_id: usize, counts: &mut [usize]) -> usize {
        let node = &nodes[node_id];
        let mut total = node.token_ids.len();
        for &child_id in node.children.values() {
            total += rec(nodes, child_id, counts);
        }
        counts[node_id] = total;
        total
    }
    let mut counts = vec![0usize; nodes.len()];
    rec(nodes, 0, &mut counts);
    counts
}

fn initial_frontier(start: usize) -> (StackInterner, FastHashSet<SearchState>) {
    let mut stacks = StackInterner::new();
    let start_stack = stacks.intern_slice(&[start]);
    let frontier = [
        SearchState {
            in_stack: 0,
            out_stack: 0,
            prev_symbol: None,
            allow_bt: true,
        },
        SearchState {
            in_stack: start_stack,
            out_stack: start_stack,
            prev_symbol: None,
            allow_bt: false,
        },
    ]
    .into_iter()
    .collect();
    (stacks, frontier)
}

fn advance_frontier(
    frontier: &FastHashSet<SearchState>,
    byte: u8,
    ig: &IntGrammar,
    stacks: &mut StackInterner,
) -> FastHashSet<SearchState> {
    let pts = &ig.preterms_rev[byte as usize];
    if pts.is_empty() {
        return fast_hash_set();
    }
    let mut next_states = fast_hash_set();
    for state in frontier {
        if stacks.is_empty(state.out_stack) {
            if !state.allow_bt {
                continue;
            }
            for &pt in pts {
                if let Some(starts) = ig.nt_map.get(pt) {
                    for &s in starts {
                        if ig.stack_adj[s].contains(&state.prev_symbol) {
                            if let Some(outs) = ig.transitions[pt].get(&s) {
                                for t0_out in outs {
                                    let ins = stacks.push(state.in_stack, s);
                                    let out_stack = stacks.intern_slice(t0_out);
                                    next_states.insert(SearchState {
                                        in_stack: ins,
                                        out_stack,
                                        prev_symbol: Some(s),
                                        allow_bt: state.allow_bt,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let head = stacks.first(state.out_stack).unwrap();
            for &pt in pts {
                if let Some(outs) = ig.transitions[pt].get(&head) {
                    for t0_out in outs {
                        let new_out = stacks.prepend_to_tail(t0_out, state.out_stack, 1);
                        next_states.insert(SearchState {
                            in_stack: state.in_stack,
                            out_stack: new_out,
                            prev_symbol: Some(head),
                            allow_bt: state.allow_bt,
                        });
                    }
                }
            }
        }
    }
    next_states
}

fn frontier_stack_pairs(frontier: &FastHashSet<SearchState>) -> Option<Vec<(usize, usize)>> {
    let mut out = frontier
        .iter()
        .map(|s| (s.in_stack, s.out_stack))
        .collect::<FastHashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    out.sort_unstable();
    (!out.is_empty()).then_some(out)
}

fn collect_token_trie_results(
    nodes: &[TokenTrieNode],
    node_id: usize,
    frontier: &FastHashSet<SearchState>,
    stacks: &mut StackInterner,
    ig: &IntGrammar,
    out: &mut Vec<LocalTokenResult>,
) {
    let node = &nodes[node_id];
    let stack_pairs = if node.token_ids.is_empty() {
        None
    } else {
        frontier_stack_pairs(frontier)
    };
    for &id in &node.token_ids {
        out.push(LocalTokenResult {
            token_id: id,
            stack_pairs: stack_pairs.clone(),
        });
    }
    for (&byte, &child_id) in &node.children {
        let next = advance_frontier(frontier, byte, ig, stacks);
        if !next.is_empty() {
            collect_token_trie_results(nodes, child_id, &next, stacks, ig, out);
        } else {
            collect_invalid_subtree(nodes, child_id, out);
        }
    }
}

fn collect_invalid_subtree(
    nodes: &[TokenTrieNode],
    node_id: usize,
    out: &mut Vec<LocalTokenResult>,
) {
    let node = &nodes[node_id];
    for &id in &node.token_ids {
        out.push(LocalTokenResult {
            token_id: id,
            stack_pairs: None,
        });
    }
    for &child_id in node.children.values() {
        collect_invalid_subtree(nodes, child_id, out);
    }
}

fn collect_token_trie_jobs(
    nodes: &[TokenTrieNode],
    subtree_token_counts: &[usize],
    node_id: usize,
    frontier: FastHashSet<SearchState>,
    stacks: &mut StackInterner,
    depth: usize,
    ig: &IntGrammar,
    immediate: &mut Vec<LocalTokenResult>,
    jobs: &mut Vec<TrieJob>,
) {
    const MAX_JOB_TOKENS: usize = 512;
    const MAX_SPLIT_DEPTH: usize = 8;

    if subtree_token_counts[node_id] <= MAX_JOB_TOKENS || depth >= MAX_SPLIT_DEPTH {
        let (rebased_stacks, rebased_frontier) = rebase_frontier(&frontier, stacks);
        jobs.push(TrieJob {
            node_id,
            frontier: rebased_frontier,
            stacks: rebased_stacks,
        });
        return;
    }

    let node = &nodes[node_id];
    let stack_pairs = frontier_stack_pairs(&frontier);
    for &id in &node.token_ids {
        immediate.push(LocalTokenResult {
            token_id: id,
            stack_pairs: stack_pairs.clone(),
        });
    }

    for (&byte, &child_id) in &node.children {
        let next = advance_frontier(&frontier, byte, ig, stacks);
        if next.is_empty() {
            collect_invalid_subtree(nodes, child_id, immediate);
        } else {
            collect_token_trie_jobs(
                nodes,
                subtree_token_counts,
                child_id,
                next,
                stacks,
                depth + 1,
                ig,
                immediate,
                jobs,
            );
        }
    }
}

fn compute_stack_in_out_for_trie(
    tasks: &[(Vec<u8>, usize)],
    ig: &IntGrammar,
    pb: Option<&ProgressBar>,
) -> TokenTraversalResults {
    let trie = build_token_trie(tasks);
    let subtree_token_counts = compute_subtree_token_counts(&trie);
    let (mut root_stacks, root_frontier) = initial_frontier(ig.start);
    let mut immediate_results = Vec::new();
    let mut jobs = Vec::new();
    for &id in &trie[0].token_ids {
        immediate_results.push(LocalTokenResult {
            token_id: id,
            stack_pairs: None,
        });
    }
    for (&byte, &child_id) in &trie[0].children {
        let frontier = advance_frontier(&root_frontier, byte, ig, &mut root_stacks);
        if frontier.is_empty() {
            collect_invalid_subtree(&trie, child_id, &mut immediate_results);
        } else {
            collect_token_trie_jobs(
                &trie,
                &subtree_token_counts,
                child_id,
                frontier,
                &mut root_stacks,
                1,
                ig,
                &mut immediate_results,
                &mut jobs,
            );
        }
    }
    if let Some(pb) = pb {
        pb.inc(immediate_results.len() as u64);
    }
    let job_results = jobs
        .into_par_iter()
        .map(|mut job| {
            let mut out = Vec::new();
            let frontier = job.frontier;
            collect_token_trie_results(
                &trie,
                job.node_id,
                &frontier,
                &mut job.stacks,
                ig,
                &mut out,
            );
            if let Some(pb) = pb {
                pb.inc(out.len() as u64);
            }
            (job.stacks, out)
        })
        .collect::<Vec<_>>();

    let mut stack_sets = vec![root_stacks];
    let mut results = immediate_results
        .into_iter()
        .map(|result| TokenStackResult {
            token_id: result.token_id,
            stack_set: 0,
            stack_pairs: result.stack_pairs,
        })
        .collect::<Vec<_>>();
    for (stacks, job) in job_results {
        let stack_set = stack_sets.len();
        stack_sets.push(stacks);
        results.extend(job.into_iter().map(|result| TokenStackResult {
            token_id: result.token_id,
            stack_set,
            stack_pairs: result.stack_pairs,
        }));
    }
    results.sort_unstable_by_key(|result| result.token_id);
    TokenTraversalResults {
        stack_sets,
        tokens: results,
    }
}

fn compute_token_classes(
    grammar: &Grammar,
    preterms: &Preterms,
    tokens: &[(Vec<u8>, usize)],
    eos: usize,
    n_logits: usize,
    args: &Args,
) -> Result<EquivOut> {
    let mut all = BTreeSet::new();
    all.extend(grammar.keys().cloned());
    for prods in grammar.values() {
        for rhs in prods {
            all.extend(rhs.iter().cloned());
        }
    }
    let mut symbols = BTreeMap::from([("S".to_string(), 0usize)]);
    for s in all.into_iter().filter(|s| s != "S") {
        let id = symbols.len();
        symbols.insert(s, id);
    }
    let n_symbols = symbols.len();
    let mut g_int: HashMap<usize, Vec<Vec<usize>>> = HashMap::new();
    for (a, prods) in grammar {
        let aid = symbols[a];
        for beta in prods {
            g_int
                .entry(aid)
                .or_default()
                .push(beta.iter().map(|b| symbols[b]).collect());
        }
    }
    let stack_adj = compute_stack_adj(&g_int, 0);
    let token_chars: HashSet<u8> = tokens.iter().flat_map(|(t, _)| t.iter().copied()).collect();
    let mut preterms_rev: [Vec<usize>; 256] = std::array::from_fn(|_| Vec::new());
    for (k, bytes) in preterms {
        let Some(&kid) = symbols.get(k) else {
            continue;
        };
        for &b in bytes {
            if token_chars.contains(&b) {
                preterms_rev[b as usize].push(kid);
            }
        }
    }
    let mut nt_map = vec![Vec::new(); n_symbols];
    let mut transitions: Vec<FastHashMap<usize, Vec<Vec<usize>>>> =
        (0..n_symbols).map(|_| fast_hash_map()).collect();
    for (a, prods) in &g_int {
        for beta in prods {
            if let Some((&b, rest)) = beta.split_first() {
                transitions[b].entry(*a).or_default().push(rest.to_vec());
                nt_map[b].push(*a);
            }
        }
    }
    let mut stack_adj_vec: Vec<FastHashSet<Option<usize>>> =
        (0..n_symbols).map(|_| fast_hash_set()).collect();
    for (sym, adj) in stack_adj {
        if sym < stack_adj_vec.len() {
            stack_adj_vec[sym] = adj;
        }
    }
    let ig = IntGrammar {
        preterms_rev,
        nt_map,
        transitions,
        stack_adj: stack_adj_vec,
        start: 0,
    };

    let ignore = |id: usize| {
        args.ignore_range
            .iter()
            .any(|(a, b)| (*a..*b).contains(&id))
    };
    let skip = |tok: &[u8]| {
        (args.skip_null_bytes && tok.contains(&0))
            || (args.skip_repeat_bytes
                && tok
                    .windows(3)
                    .any(|w| matches!(w, [42, 42, 42] | [43, 43, 43] | [45, 45, 45])))
    };
    let mut tasks = Vec::new();
    let mut skip_classes = Vec::new();
    let mut tokens_sorted = tokens.to_vec();
    tokens_sorted.sort_by_key(|(_, id)| *id);
    for (tok, id) in &tokens_sorted {
        if ignore(*id) || *id == eos {
            continue;
        }
        if skip(tok) {
            skip_classes.push(*id);
        } else {
            tasks.push((tok.clone(), *id));
        }
    }

    let pb = if !args.no_progress {
        let pb = ProgressBar::new(tasks.len() as u64);
        pb.set_style(progress_style(
            "preprocessing tokens [{elapsed_precise}] {wide_bar} {pos}/{len} ({per_sec}, eta {eta})",
        ));
        Some(pb)
    } else {
        None
    };
    let results = compute_stack_in_out_for_trie(&tasks, &ig, pb.as_ref());
    if let Some(pb) = &pb {
        pb.finish();
    }

    let bucket_pb = if !args.no_progress {
        let pb = ProgressBar::new(results.tokens.len() as u64);
        pb.set_style(progress_style(
            "bucketing classes [{elapsed_precise}] {wide_bar} {pos}/{len} ({per_sec}, eta {eta})",
        ));
        Some(pb)
    } else {
        None
    };
    let mut global_stack_ids: FastHashMap<Vec<usize>, usize> = fast_hash_map();
    let mut seq_ids: FastHashMap<(usize, usize), usize> = fast_hash_map();
    let mut classes: FastHashMap<Vec<usize>, Vec<usize>> = fast_hash_map();
    let mut invalid = Vec::new();
    for result in results.tokens {
        if let Some(stack_pairs) = result.stack_pairs {
            let stacks = &results.stack_sets[result.stack_set];
            let mut disp = Vec::new();
            for (in_stack, out_stack) in stack_pairs {
                let seq = (
                    intern_global_stack(&mut global_stack_ids, &stacks.stacks[in_stack]),
                    intern_global_stack(&mut global_stack_ids, &stacks.stacks[out_stack]),
                );
                let next_id = seq_ids.len();
                disp.push(*seq_ids.entry(seq).or_insert(next_id));
            }
            disp.sort_unstable();
            classes.entry(disp).or_default().push(result.token_id);
        } else {
            invalid.push(result.token_id as i32);
        }
        if let Some(pb) = &bucket_pb {
            pb.inc(1);
        }
    }
    if let Some(pb) = &bucket_pb {
        pb.finish();
    }
    let mut token_classes: Vec<Vec<usize>> = skip_classes.into_iter().map(|x| vec![x]).collect();
    let mut grouped_classes = classes.into_values().collect::<Vec<_>>();
    grouped_classes.sort_unstable_by_key(|class| class.iter().min().copied().unwrap_or(usize::MAX));
    token_classes.extend(grouped_classes);
    token_classes.push(vec![eos]);
    let mut vec_out = vec![-1i32; n_logits];
    let mut tok_rev = vec![None; n_logits];
    for (tok, id) in tokens {
        if *id < tok_rev.len() {
            tok_rev[*id] = Some(tok);
        }
    }
    let mut reps = vec![Vec::<u8>::new(); token_classes.len()];
    for (i, class) in token_classes.iter().enumerate() {
        let mut best: Option<&Vec<u8>> = None;
        for &id in class {
            if id < vec_out.len() {
                vec_out[id] = i as i32;
            }
            if let Some(tok) = tok_rev.get(id).and_then(|tok| *tok) {
                if best.as_ref().map_or(true, |b| tok.len() < b.len()) {
                    best = Some(tok);
                }
            }
        }
        reps[i] = best.cloned().unwrap_or_default();
    }
    Ok(EquivOut {
        token_classes: vec_out,
        invalid_tokens: invalid,
        class_representatives: reps,
    })
}

fn write_output(path: &Path, eq: &EquivOut, show_progress: bool) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!("output exists and is not a directory: {}", path.display());
        }
        if path.read_dir()?.next().is_some() {
            bail!("output directory is not empty: {}", path.display());
        }
    } else {
        fs::create_dir_all(path)?;
    }
    let reps_bytes = eq
        .class_representatives
        .iter()
        .map(|rep| rep.len())
        .sum::<usize>();
    let write_units = (eq.token_classes.len() + eq.invalid_tokens.len())
        * std::mem::size_of::<i32>()
        + reps_bytes;
    let pb = if show_progress {
        let pb = ProgressBar::new(write_units as u64);
        pb.set_style(progress_style(
            "writing output [{elapsed_precise}] {wide_bar} {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
        ));
        Some(pb)
    } else {
        None
    };
    write_raw_i32(path.join("tc.pt"), &eq.token_classes, pb.as_ref())?;
    write_raw_i32(path.join("inv.pt"), &eq.invalid_tokens, pb.as_ref())?;
    let mut f = BufWriter::new(File::create(path.join("cr.pkl"))?);
    let reps: Vec<serde_bytes::ByteBuf> = eq
        .class_representatives
        .iter()
        .cloned()
        .map(serde_bytes::ByteBuf::from)
        .collect();
    serde_pickle::to_writer(&mut f, &reps, Default::default())?;
    if let Some(pb) = &pb {
        pb.inc(reps_bytes as u64);
        pb.finish();
    }
    Ok(())
}

fn write_raw_i32(path: PathBuf, xs: &[i32], pb: Option<&ProgressBar>) -> Result<()> {
    let root = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tensor")
        .to_string();
    let f = File::create(path)?;
    let mut zip = zip::ZipWriter::new(BufWriter::new(f));
    let opts =
        zip::write::FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(format!("{root}/data.pkl"), opts)?;
    zip.write_all(&torch_int_tensor_pickle(xs.len()))?;
    zip.start_file(format!("{root}/.format_version"), opts)?;
    zip.write_all(b"1")?;
    zip.start_file(format!("{root}/.storage_alignment"), opts)?;
    zip.write_all(b"64")?;
    zip.start_file(format!("{root}/byteorder"), opts)?;
    zip.write_all(b"little")?;
    zip.start_file(format!("{root}/data/0"), opts)?;
    let mut buf = Vec::with_capacity(64 * 1024);
    for chunk in xs.chunks(16 * 1024) {
        buf.clear();
        for &x in chunk {
            buf.extend_from_slice(&x.to_le_bytes());
        }
        zip.write_all(&buf)?;
        if let Some(pb) = pb {
            pb.inc(buf.len() as u64);
        }
    }
    zip.start_file(format!("{root}/version"), opts)?;
    zip.write_all(b"3\n")?;
    zip.start_file(format!("{root}/.data/serialization_id"), opts)?;
    zip.write_all(b"0000000000000000000000000000000000000000")?;
    zip.finish()?;
    Ok(())
}

fn torch_int_tensor_pickle(len: usize) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(b"\x80\x02");
    pickle_global(&mut p, "torch._utils", "_rebuild_tensor_v2");
    pickle_binput(&mut p, 0);
    p.push(b'(');
    p.push(b'(');
    pickle_binunicode(&mut p, "storage");
    pickle_binput(&mut p, 1);
    pickle_global(&mut p, "torch", "IntStorage");
    pickle_binput(&mut p, 2);
    pickle_binunicode(&mut p, "0");
    pickle_binput(&mut p, 3);
    pickle_binunicode(&mut p, "cpu");
    pickle_binput(&mut p, 4);
    pickle_binint(&mut p, len as i32);
    p.push(b't');
    pickle_binput(&mut p, 5);
    p.push(b'Q');
    pickle_binint(&mut p, 0);
    pickle_binint(&mut p, len as i32);
    p.push(0x85);
    pickle_binput(&mut p, 6);
    pickle_binint(&mut p, 1);
    p.push(0x85);
    pickle_binput(&mut p, 7);
    p.push(0x89);
    pickle_global(&mut p, "collections", "OrderedDict");
    pickle_binput(&mut p, 8);
    p.push(b')');
    p.push(b'R');
    pickle_binput(&mut p, 9);
    p.push(b't');
    pickle_binput(&mut p, 10);
    p.push(b'R');
    pickle_binput(&mut p, 11);
    p.push(b'.');
    p
}

fn pickle_global(out: &mut Vec<u8>, module: &str, name: &str) {
    out.push(b'c');
    out.extend_from_slice(module.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(name.as_bytes());
    out.push(b'\n');
}

fn pickle_binput(out: &mut Vec<u8>, id: u8) {
    out.push(b'q');
    out.push(id);
}

fn pickle_binunicode(out: &mut Vec<u8>, s: &str) {
    out.push(b'X');
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn pickle_binint(out: &mut Vec<u8>, x: i32) {
    out.push(b'J');
    out.extend_from_slice(&x.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_args() -> Args {
        Args {
            model_id: "test-model".to_string(),
            grammar_file: PathBuf::from("grammar.gbnf"),
            output: PathBuf::from("out"),
            start_symbol: "root".to_string(),
            n_logits: None,
            ignore_range: Vec::new(),
            skip_null_bytes: false,
            skip_repeat_bytes: false,
            num_threads: 1,
            hf_token: None,
            cache_dir: None,
            no_progress: true,
        }
    }

    fn tok(s: &str, id: usize) -> (Vec<u8>, usize) {
        (s.as_bytes().to_vec(), id)
    }

    fn tok_bytes(bytes: &[u8], id: usize) -> (Vec<u8>, usize) {
        (bytes.to_vec(), id)
    }

    fn classify(grammar: &str, tokens: Vec<(Vec<u8>, usize)>, eos: usize) -> EquivOut {
        classify_with_args(grammar, tokens, eos, test_args())
    }

    fn classify_with_args(
        grammar: &str,
        tokens: Vec<(Vec<u8>, usize)>,
        eos: usize,
        args: Args,
    ) -> EquivOut {
        let (cfg, nfa, preterms, labels) = parse_cfg_str(grammar, &args.start_symbol).unwrap();
        let normed = normalize_cfg(cfg, nfa, labels).unwrap();
        let n_logits = tokens
            .iter()
            .map(|(_, id)| *id)
            .chain(std::iter::once(eos))
            .max()
            .unwrap()
            + 1;
        compute_token_classes(&normed, &preterms, &tokens, eos, n_logits, &args).unwrap()
    }

    fn normalized(grammar: &str) -> (Grammar, Preterms) {
        let (cfg, nfa, preterms, labels) = parse_cfg_str(grammar, "root").unwrap();
        (normalize_cfg(cfg, nfa, labels).unwrap(), preterms)
    }

    fn class_of(eq: &EquivOut, id: usize) -> i32 {
        eq.token_classes[id]
    }

    fn assert_valid(eq: &EquivOut, id: usize) {
        assert!(
            class_of(eq, id) >= 0,
            "token {id} should have a class; invalid={:?}, classes={:?}",
            eq.invalid_tokens,
            eq.token_classes
        );
        assert!(
            !eq.invalid_tokens.contains(&(id as i32)),
            "token {id} should not be invalid"
        );
    }

    fn assert_invalid(eq: &EquivOut, id: usize) {
        assert_eq!(class_of(eq, id), -1, "token {id} should be unassigned");
        assert!(
            eq.invalid_tokens.contains(&(id as i32)),
            "token {id} should be listed as invalid; invalid={:?}",
            eq.invalid_tokens
        );
    }

    fn matches(re: &str, s: &str) -> bool {
        regex::Regex::new(&format!("^(?:{re})$"))
            .unwrap()
            .is_match(s)
    }

    #[test]
    fn gbnf_regex_basics() {
        assert_eq!(gbnf_to_regex("\"abc\"").unwrap(), "abc");
        assert!(matches(&gbnf_to_regex("\"a.b\"").unwrap(), "a.b"));
        assert!(!matches(&gbnf_to_regex("\"a.b\"").unwrap(), "axb"));
        assert!(matches(&gbnf_to_regex("[a-z]+").unwrap(), "abc"));
        assert!(!matches(&gbnf_to_regex("[a-z]+").unwrap(), "ABC"));
    }

    #[test]
    fn parse_terminal_reuse() {
        let (terms, prods) = parse_gbnf("root ::= \"a\" \"a\"").unwrap();
        assert_eq!(terms.len(), 1);
        let t = terms.keys().next().unwrap().clone();
        assert_eq!(prods["root"], BTreeSet::from([vec![t]]));
    }

    #[test]
    fn parse_nonterminal_quant() {
        let (_terms, prods) = parse_gbnf("root ::= item+\nitem ::= \"x\"").unwrap();
        let aux = prods["root"].iter().next().unwrap()[0].clone();
        assert!(prods[&aux].contains(&vec!["item".to_string()]));
        assert!(prods[&aux].iter().any(|rhs| rhs.contains(&aux)));
    }

    #[test]
    fn parser_handles_comments_multiline_groups_escapes_and_start_symbols() {
        let grammar = r#"
            # whole-line comments disappear
            entry ::= (
                "a#not-comment" [#b] "\n"
                | item? "\t"
            ) # inline comments disappear
            item ::= "x\"y" | "\\"
        "#;
        let (terms, prods) = parse_gbnf(grammar).unwrap();
        assert!(prods.contains_key("entry"));
        assert!(prods.contains_key("item"));
        assert!(terms.values().any(|rx| matches(rx, "a#not-comment#\n")));
        assert!(terms.values().any(|rx| matches(rx, "\t")));
        assert!(parse_cfg_str(grammar, "entry").is_ok());

        let err = parse_cfg_str(grammar, "root").unwrap_err().to_string();
        assert!(err.contains("start symbol"));
        assert!(err.contains("entry"));
    }

    #[test]
    fn parser_closure_holds_for_mixed_alternation_and_group_quantifiers() {
        let grammar = r#"
            root ::= ("pre" item | alt)+ "!"
            item ::= [a-z]+
            alt ::= "x" | "y"
        "#;
        let (terms, prods) = parse_gbnf(grammar).unwrap();
        let known_nt = prods.keys().cloned().collect::<BTreeSet<_>>();
        let known_t = terms.keys().cloned().collect::<BTreeSet<_>>();
        for (lhs, rhss) in &prods {
            for rhs in rhss {
                for sym in rhs {
                    assert!(
                        known_nt.contains(sym) || known_t.contains(sym),
                        "unknown symbol {sym:?} in {lhs} -> {rhs:?}"
                    );
                }
            }
        }
        assert!(prods["root"].iter().any(|rhs| rhs.len() == 2));
    }

    #[test]
    fn normalization_removes_epsilon_unary_unreachable_and_temporary_symbols() {
        let grammar = r#"
            root ::= a
            a ::= b
            b ::= c?
            c ::= "x"
            dead ::= "z"
        "#;
        let (normed, preterms) = normalized(grammar);
        assert!(normed.contains_key("S"));
        assert!(!normed.contains_key("dead"));
        assert!(normed.keys().all(|k| !k.starts_with("PT[")));
        assert!(normed
            .values()
            .all(|rhss| rhss.iter().all(|rhs| !rhs.is_empty())));
        assert!(normed
            .values()
            .flat_map(|rhss| rhss.iter())
            .all(|rhs| preterms.contains_key(&rhs[0])));
    }

    #[test]
    fn normalization_handles_direct_and_indirect_left_recursion() {
        let grammar = r#"
            root ::= expr
            expr ::= expr "+" atom | term
            term ::= expr "-" atom | atom
            atom ::= [0-9]+
        "#;
        let (normed, preterms) = normalized(grammar);
        assert!(normed.contains_key("S"));
        assert!(normed.keys().any(|k| k.starts_with("LR[")));
        for (lhs, rhss) in &normed {
            for rhs in rhss {
                assert!(!rhs.is_empty(), "{lhs} has an empty production");
                assert!(
                    preterms.contains_key(&rhs[0]),
                    "{lhs} -> {rhs:?} is not in GNF byte-preterminal shape"
                );
            }
        }
    }

    #[test]
    fn prefix_tokens_exercise_partial_and_invalid_suffixes() {
        let eq = classify(
            r#"root ::= "a" "b""#,
            vec![
                tok("a", 0),
                tok("ab", 1),
                tok("abc", 2),
                tok("b", 3),
                tok("c", 4),
                tok("<eos>", 5),
            ],
            5,
        );
        assert_valid(&eq, 0);
        assert_valid(&eq, 1);
        assert_valid(&eq, 3);
        assert_invalid(&eq, 2);
        assert_invalid(&eq, 4);
        assert_valid(&eq, 5);
        assert_ne!(class_of(&eq, 5), class_of(&eq, 1));
    }

    #[test]
    fn shared_prefix_keywords_keep_partial_tokens_distinct_from_full_tokens() {
        let eq = classify(
            r#"root ::= "true" | "false""#,
            vec![
                tok("t", 0),
                tok("tr", 1),
                tok("true", 2),
                tok("f", 3),
                tok("fa", 4),
                tok("false", 5),
                tok("x", 6),
                tok("<eos>", 7),
            ],
            7,
        );
        for id in 0..=5 {
            assert_valid(&eq, id);
        }
        assert_invalid(&eq, 6);
        assert_ne!(class_of(&eq, 0), class_of(&eq, 2));
        assert_ne!(class_of(&eq, 3), class_of(&eq, 5));
    }

    #[test]
    fn nullable_branches_do_not_overaccept_impossible_tokens() {
        let eq = classify(
            r#"
                root ::= sign? ws? digits
                sign ::= "+" | "-"
                ws ::= " "
                digits ::= [0-9]+
            "#,
            vec![
                tok("+", 0),
                tok("-", 1),
                tok(" ", 2),
                tok("1", 3),
                tok("+1", 4),
                tok("- 2", 5),
                tok("++", 6),
                tok("a", 7),
                tok("<eos>", 8),
            ],
            8,
        );
        for id in 0..=5 {
            assert_valid(&eq, id);
        }
        assert_invalid(&eq, 6);
        assert_invalid(&eq, 7);
    }

    #[test]
    fn recursive_arithmetic_tokens_preserve_stack_contexts() {
        let eq = classify(
            r#"
                root ::= expr
                expr ::= term (("+" | "-") term)*
                term ::= factor (("*" | "/") factor)*
                factor ::= [0-9]+ | "(" expr ")"
            "#,
            vec![
                tok("1", 0),
                tok("12", 1),
                tok("+", 2),
                tok("+3", 3),
                tok("(", 4),
                tok("(1", 5),
                tok(")", 6),
                tok("*", 7),
                tok("a", 8),
                tok("<eos>", 9),
            ],
            9,
        );
        for id in 0..=7 {
            assert_valid(&eq, id);
        }
        assert_invalid(&eq, 8);
        assert_ne!(class_of(&eq, 0), class_of(&eq, 2));
        assert_ne!(class_of(&eq, 4), class_of(&eq, 6));
    }

    #[test]
    fn indirect_recursion_and_dead_rules_respect_stack_adjacency_and_reachability() {
        let eq = classify(
            r#"
                root ::= a
                a ::= b "x" | "a"
                b ::= a "y" | "b"
                dead ::= "z"
            "#,
            vec![
                tok("a", 0),
                tok("b", 1),
                tok("x", 2),
                tok("y", 3),
                tok("ax", 4),
                tok("by", 5),
                tok("z", 6),
                tok("<eos>", 7),
            ],
            7,
        );
        for id in 0..=3 {
            assert_valid(&eq, id);
        }
        assert_invalid(&eq, 4);
        assert_invalid(&eq, 5);
        assert_invalid(&eq, 6);
    }

    #[test]
    fn character_classes_cover_ranges_and_negation() {
        let eq = classify(
            r#"
                root ::= word " " number " " not_newline
                word ::= [a-z]+
                number ::= [0-9]+
                not_newline ::= [^\n]+
            "#,
            vec![
                tok("abc", 0),
                tok("9", 1),
                tok(" ", 2),
                tok("Z", 3),
                tok("\n", 4),
                tok("hello 42 ok", 5),
                tok("<eos>", 6),
                tok("A\n", 7),
            ],
            6,
        );
        assert_valid(&eq, 0);
        assert_valid(&eq, 1);
        assert_valid(&eq, 2);
        assert_valid(&eq, 3);
        assert_valid(&eq, 5);
        assert_invalid(&eq, 4);
        assert_invalid(&eq, 7);
    }

    #[test]
    fn utf8_literals_and_split_bytes_are_classified() {
        let lambda = "λ".as_bytes();
        let e_acute = "é".as_bytes();
        let eq = classify(
            r#"root ::= "é" | "λ""#,
            vec![
                tok("é", 0),
                tok("λ", 1),
                tok_bytes(&lambda[..1], 2),
                tok_bytes(&lambda[1..], 3),
                tok_bytes(&e_acute[..1], 4),
                tok("e", 5),
                tok("<eos>", 6),
            ],
            6,
        );
        assert_valid(&eq, 0);
        assert_valid(&eq, 1);
        assert_valid(&eq, 2);
        assert_valid(&eq, 3);
        assert_valid(&eq, 4);
        assert_invalid(&eq, 5);
    }

    #[test]
    fn control_bytes_skip_options_and_ignored_ranges_are_honored() {
        let mut args = test_args();
        args.skip_null_bytes = true;
        args.skip_repeat_bytes = true;
        args.ignore_range = vec![(6, 7)];
        let eq = classify_with_args(
            r#"root ::= "a" | "\n" | "\t" | "\x00" | "***""#,
            vec![
                tok("a", 0),
                tok("\n", 1),
                tok("\t", 2),
                tok_bytes(&[0], 3),
                tok("***", 4),
                tok("b", 5),
                tok("ignored", 6),
                tok("<eos>", 7),
            ],
            7,
            args,
        );
        assert_valid(&eq, 0);
        assert_valid(&eq, 1);
        assert_valid(&eq, 2);
        assert_valid(&eq, 3);
        assert_valid(&eq, 4);
        assert_invalid(&eq, 5);
        assert_eq!(class_of(&eq, 6), -1);
        assert!(!eq.invalid_tokens.contains(&6));
        assert_ne!(class_of(&eq, 3), class_of(&eq, 4));
    }

    #[test]
    fn duplicate_state_compression_groups_language_equivalent_tokens() {
        let eq = classify(
            r#"
                root ::= left | right
                left ::= "a" tail
                right ::= "b" tail
                tail ::= "!"
            "#,
            vec![
                tok("a", 0),
                tok("b", 1),
                tok("!", 2),
                tok("a!", 3),
                tok("b!", 4),
                tok("?", 5),
                tok("<eos>", 6),
            ],
            6,
        );
        assert_valid(&eq, 0);
        assert_valid(&eq, 1);
        assert_valid(&eq, 2);
        assert_valid(&eq, 3);
        assert_valid(&eq, 4);
        assert_invalid(&eq, 5);
        assert_eq!(class_of(&eq, 0), class_of(&eq, 1));
        assert_eq!(class_of(&eq, 3), class_of(&eq, 4));
    }

    #[test]
    fn representatives_choose_shortest_token_in_class_and_eos_is_singleton() {
        let eq = classify(
            r#"root ::= [a-z]+"#,
            vec![
                tok("a", 0),
                tok("b", 1),
                tok("ab", 2),
                tok("abc", 3),
                tok("<eos>", 4),
            ],
            4,
        );
        for id in 0..=3 {
            assert_valid(&eq, id);
        }
        assert_valid(&eq, 4);
        assert_ne!(class_of(&eq, 4), class_of(&eq, 0));
        let class = class_of(&eq, 0) as usize;
        assert_eq!(eq.class_representatives[class], b"a");
    }

    #[test]
    fn output_serialization_smoke_test_and_path_errors() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("artifacts");
        let eq = EquivOut {
            token_classes: vec![0, -1, 1],
            invalid_tokens: vec![1],
            class_representatives: vec![b"a".to_vec(), b"<eos>".to_vec()],
        };
        write_output(&out_dir, &eq, false).unwrap();
        assert!(out_dir.join("tc.pt").is_file());
        assert!(out_dir.join("inv.pt").is_file());
        assert!(out_dir.join("cr.pkl").is_file());
        assert!(write_output(&out_dir, &eq, false).is_err());

        let file_path = dir.path().join("already-a-file");
        fs::write(&file_path, b"x").unwrap();
        assert!(write_output(&file_path, &eq, false).is_err());
    }

    #[test]
    fn token_trie_reuses_prefix_frontiers() {
        let mut preterms_rev: [Vec<usize>; 256] = std::array::from_fn(|_| Vec::new());
        preterms_rev[b'a' as usize] = vec![1];
        let mut transitions: Vec<FastHashMap<usize, Vec<Vec<usize>>>> =
            (0..2).map(|_| fast_hash_map()).collect();
        transitions[1].insert(0, vec![Vec::new(), vec![0]]);
        let ig = IntGrammar {
            preterms_rev,
            nt_map: vec![Vec::new(), vec![0]],
            transitions,
            stack_adj: (0..2).map(|_| fast_hash_set()).collect(),
            start: 0,
        };
        let tasks = vec![(vec![b'a'], 10), (vec![b'a', b'a'], 11), (vec![b'b'], 12)];
        let traversal = compute_stack_in_out_for_trie(&tasks, &ig, None);
        let got = traversal
            .tokens
            .iter()
            .map(|result| {
                let materialized = result.stack_pairs.as_ref().map(|pairs| {
                    let stacks = &traversal.stack_sets[result.stack_set];
                    pairs
                        .iter()
                        .map(|(in_stack, out_stack)| {
                            (
                                stacks.stacks[*in_stack].clone(),
                                stacks.stacks[*out_stack].clone(),
                            )
                        })
                        .collect::<HashSet<_>>()
                });
                (result.token_id, materialized)
            })
            .collect::<HashMap<_, _>>();

        let one_a = HashSet::from([(vec![0], Vec::new()), (vec![0], vec![0])]);
        let two_a = HashSet::from([(vec![0], Vec::new()), (vec![0], vec![0])]);
        assert_eq!(got.get(&10), Some(&Some(one_a)));
        assert_eq!(got.get(&11), Some(&Some(two_a)));
        assert_eq!(got.get(&12), Some(&None));
    }
}
