use super::*;

fn attr(out: &mut String, key: &str, value: &str) {
    write!(out, " {key}=\"{}\"", escape_attribute(value)).unwrap();
}
fn open(out: &mut String, depth: usize, tag: &str) {
    write!(out, "{}<{tag}", "  ".repeat(depth)).unwrap();
}
fn close(out: &mut String, depth: usize, tag: &str) {
    writeln!(out, "{}</{tag}>", "  ".repeat(depth)).unwrap();
}
fn local(l: &LocalBinding, d: usize, out: &mut String) {
    match l {
        LocalBinding::Let(v) => write_let(v, d, out),
        LocalBinding::Bind(v) => write_bind(v, d, out),
    }
}
fn condition(c: &PredicateExpression, d: usize, out: &mut String) {
    let (tag, children) = match c {
        PredicateExpression::Call { name, arguments } => {
            open(out, d, name);
            for (name, value) in arguments {
                attr(out, name, value);
            }
            out.push_str("/>\n");
            return;
        }
        PredicateExpression::Not(c) => ("not", std::slice::from_ref(c.as_ref())),
        PredicateExpression::All(cs) => ("all", cs.as_slice()),
        PredicateExpression::Any(cs) => ("any", cs.as_slice()),
    };
    open(out, d, tag);
    out.push_str(">\n");
    for child in children {
        condition(child, d + 1, out);
    }
    close(out, d, tag);
}
pub(super) fn predicate(p: &Predicate, out: &mut String) {
    open(out, 1, "predicate");
    attr(out, "name", &p.name);
    out.push_str(">\n");
    for member in &p.members {
        match member {
            PredicateMember::Input(v) => write_port(out, "input", v, ""),
            PredicateMember::Binding(v) => local(v, 2, out),
            PredicateMember::Predicate(v) => condition(v, 2, out),
        }
    }
    close(out, 1, "predicate");
}
pub(super) fn declare_predicate(p: &DeclarePredicate, out: &mut String) {
    open(out, 1, "declare-predicate");
    attr(out, "name", &p.name);
    out.push_str(">\n");
    for member in &p.members {
        match member {
            DeclarePredicateMember::Input(p) => write_port(out, "input", p, ""),
            DeclarePredicateMember::Error(e) => {
                open(out, 2, "error");
                attr(out, "type", e.spelling());
                out.push_str("/>\n");
            }
        }
    }
    close(out, 1, "declare-predicate");
}
fn branch(b: &Branch, d: usize, tag: &str, out: &mut String) {
    attr(out, "result", &b.exit.result);
    if let Some(ty) = &b.exit.throw {
        attr(out, "throw", ty.spelling());
    }
    if b.members.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for member in &b.members {
        match member {
            BranchMember::Binding(v) => local(v, d + 1, out),
            BranchMember::Predicate(c) => condition(c, d + 1, out),
        }
    }
    close(out, d, tag);
}
pub(super) fn select(s: &Select, d: usize, out: &mut String) {
    open(out, d, "select");
    attr(out, "type", s.result_type.spelling());
    out.push_str(">\n");
    for b in &s.cases {
        open(out, d + 1, "case");
        branch(b, d + 1, "case", out);
    }
    close(out, d, "select");
}
pub(super) fn attempt(t: &Try, d: usize, out: &mut String) {
    open(out, d, "try");
    attr(out, "type", t.result_type.spelling());
    attr(out, "result", &t.result);
    out.push_str(">\n");
    for m in &t.members {
        match m {
            TryMember::Binding(l) => local(l, d + 1, out),
            TryMember::Unreachable { reason } => {
                open(out, d + 1, "unreachable");
                if let Some(ty) = reason {
                    attr(out, "type", ty.spelling());
                }
                out.push_str("/>\n");
            }
            TryMember::Catch(c) => {
                open(out, d + 1, "catch");
                if let Some(t) = &c.reason {
                    attr(out, "type", t.spelling());
                }
                if let Some(n) = &c.name {
                    attr(out, "name", n);
                }
                branch(&c.branch, d + 1, "catch", out);
            }
        }
    }
    close(out, d, "try");
}
