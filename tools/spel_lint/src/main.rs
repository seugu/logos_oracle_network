//! Static checks for a #[lez_program] guest source, replicating the invariants
//! the spel macro + LEZ runtime enforce at build/exec time.
use std::collections::HashSet;
use syn::{Expr, FnArg, Item, ItemMod, Pat, ReturnType};

#[derive(Debug, Clone)]
struct Acc { name: String, init: bool, signer: bool, has_pda: bool }

/// Top-level flag idents inside `#[account(...)]`, ignoring anything nested
/// inside `pda = [...]`. Substring matching is wrong here: "token_definition_account"
/// contains the literal "init".
fn account_flags(attrs: &[syn::Attribute]) -> Option<Vec<String>> {
    use proc_macro2::{TokenTree, Delimiter};
    let a = attrs.iter().find(|x| x.path().is_ident("account"))?;
    let syn::Meta::List(list) = &a.meta else { return Some(vec![]) };
    let mut flags = Vec::new();
    let mut depth_skip = false;
    let mut iter = list.tokens.clone().into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Ident(id) => {
                // `name = ...` form: record the key, then skip its value
                if matches!(iter.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '=') {
                    flags.push(id.to_string());
                    iter.next(); // '='
                    // skip the value: either a group or tokens until the next top-level comma
                    while let Some(nx) = iter.peek() {
                        if matches!(nx, TokenTree::Punct(p) if p.as_char() == ',') { break }
                        iter.next();
                    }
                } else {
                    flags.push(id.to_string());
                }
            }
            TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket => { depth_skip = true; }
            _ => {}
        }
    }
    let _ = depth_skip;
    Some(flags)
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: spel_lint <file.rs>");
    let src = std::fs::read_to_string(&path).expect("read");
    let file: syn::File = match syn::parse_file(&src) {
        Ok(f) => f,
        Err(e) => { println!("FAIL parse: {e}"); std::process::exit(1); }
    };
    println!("OK  parse: {} top-level items", file.items.len());

    let prog: &ItemMod = file.items.iter().find_map(|i| match i {
        Item::Mod(m) if m.attrs.iter().any(|a| a.path().is_ident("lez_program")) => Some(m),
        _ => None,
    }).expect("no #[lez_program] module");
    println!("OK  found #[lez_program] mod {}", prog.ident);

    let items = &prog.content.as_ref().expect("empty mod").1;
    let mut failures = 0usize;

    for item in items {
        let Item::Fn(f) = item else { continue };
        if !f.attrs.iter().any(|a| a.path().is_ident("instruction")) { continue }
        let fname = f.sig.ident.to_string();

        // return type must be SpelResult
        let rt_ok = matches!(&f.sig.output, ReturnType::Type(_, t) if quote::quote!(#t).to_string() == "SpelResult");
        if !rt_ok { println!("FAIL {fname}: return type is not SpelResult"); failures += 1; }

        // collect declared accounts, in order, plus ctx / arg params
        let mut accounts: Vec<Acc> = vec![];
        let mut args: Vec<String> = vec![];
        let mut has_ctx = false;
        for input in &f.sig.inputs {
            let FnArg::Typed(pt) = input else { continue };
            let name = match &*pt.pat {
                Pat::Ident(pi) => pi.ident.to_string(),
                _ => continue,
            };
            let tystr = { let t = &pt.ty; quote::quote!(#t).to_string() };
            if tystr == "ProgramContext" { has_ctx = true; continue }
            match account_flags(&pt.attrs) {
                Some(flags) => accounts.push(Acc {
                    name,
                    init: flags.iter().any(|f| f == "init"),
                    signer: flags.iter().any(|f| f == "signer"),
                    has_pda: flags.iter().any(|f| f == "pda"),
                }),
                None => {
                    if tystr == "AccountWithMetadata" {
                        println!("FAIL {fname}: param `{name}` is an account with no #[account(..)] attribute");
                        failures += 1;
                    } else { args.push(name) }
                }
            }
        }

        // find SpelOutput::execute(vec![..], vec![..])
        let mut found = false;
        struct V { listed: Option<Vec<String>>, calls: Option<usize>, count: usize }
        impl<'ast> syn::visit::Visit<'ast> for V {
            fn visit_expr(&mut self, e: &'ast Expr) {
                if let Expr::Call(c) = e {
                    let fs = { let f = &c.func; quote::quote!(#f).to_string().replace(' ', "") };
                    if fs == "SpelOutput::execute" && c.args.len() == 2 {
                        self.count += 1;
                        if let Expr::Macro(m) = &c.args[0] {
                            if m.mac.path.is_ident("vec") {
                                let toks = m.mac.tokens.to_string();
                                self.listed = Some(toks.split(',').map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty()).collect());
                            }
                        }
                        if let Expr::Macro(m) = &c.args[1] {
                            if m.mac.path.is_ident("vec") {
                                let t = m.mac.tokens.to_string();
                                self.calls = Some(if t.trim().is_empty() { 0 }
                                    else { t.split(',').filter(|s| !s.trim().is_empty()).count() });
                            }
                        }
                    }
                }
                syn::visit::visit_expr(self, e);
            }
        }
        let mut v = V { listed: None, calls: None, count: 0 };
        syn::visit::Visit::visit_item_fn(&mut v, f);
        if v.count > 0 { found = true }
        if !found { println!("FAIL {fname}: no SpelOutput::execute(vec![..], vec![..]) call"); failures += 1; continue }

        let declared: Vec<String> = accounts.iter().map(|a| a.name.clone()).collect();
        match &v.listed {
            None => { println!("WARN {fname}: execute() accounts arg is not a plain vec![ident,..]; macro cannot resolve account PDA seeds"); }
            Some(listed) => {
                if listed.len() != declared.len() {
                    println!("FAIL {fname}: execute() lists {} accounts but {} are declared -> execute_with_claims asserts equal length (guest panic) and the runtime rejects a pre/post length mismatch",
                             listed.len(), declared.len());
                    println!("      declared: {declared:?}");
                    println!("      listed:   {listed:?}");
                    failures += 1;
                } else if *listed != declared {
                    println!("FAIL {fname}: execute() account order {listed:?} != declaration order {declared:?} (post-states are zipped positionally)");
                    failures += 1;
                } else {
                    println!("OK  {fname}: {} accounts, declaration order preserved, {} chained call(s), ctx={}",
                             declared.len(), v.calls.unwrap_or(0), has_ctx);
                }
                let uniq: HashSet<_> = listed.iter().collect();
                if uniq.len() != listed.len() {
                    println!("FAIL {fname}: duplicate account in execute() list"); failures += 1;
                }
            }
        }

        // unused declared args (the original bug: `amount` was never read)
        let body = { let b = &f.block; quote::quote!(#b).to_string() };
        for a in &args {
            let uses = body.matches(a.as_str()).count();
            if uses == 0 {
                println!("FAIL {fname}: instruction arg `{a}` is never used in the body");
                failures += 1;
            }
        }
        for a in &accounts {
            let uses = body.matches(a.name.as_str()).count();
            if uses == 0 {
                println!("FAIL {fname}: account `{}` is never used in the body", a.name);
                failures += 1;
            }
        }
        // init + pda on a non-claiming account
        for a in &accounts {
            if a.init && a.has_pda && v.calls.unwrap_or(0) > 0 {
                println!("WARN {fname}: `{}` is #[account(init, pda=..)] while the instruction also emits chained calls — double claim risks InconsistentAccountPreState", a.name);
            }
            let _ = a.signer;
        }
    }

    println!("\n{}", if failures == 0 { "PASS — no failures".into() } else { format!("{failures} FAILURE(S)") });
    if failures > 0 { std::process::exit(1) }
}
