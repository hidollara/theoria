use super::*;

fn plain(kind: &TypeName) -> ValueType {
    ValueType {
        kind: kind.clone(),
        errors: vec![],
    }
}

impl Resolver<'_> {
    pub(super) fn argument(
        &mut self,
        text: &str,
        expected: &ValueType,
        scope: &mut Scope,
    ) -> Result<Argument, Error> {
        if !matches!(
            self.definitions.get(expected.kind.spelling()),
            Some(syntax::Definition::Element(_))
        ) {
            return Ok(Argument::Attribute {
                text: text.into(),
                expected: expected.clone(),
            });
        }
        let name: crate::identifier::Identifier = text.parse()?;
        let actual = self.binding(name.as_str(), scope)?;
        // A reason outside the receiving port's contract escapes at the call site.
        // Effect inference tracks it independently of the successful value's type.
        if !self.satisfies(&actual, &expected.kind) {
            return Err(Error(format!(
                "expected {}, got {}",
                expected.spelling(),
                actual.spelling()
            )));
        }
        Ok(Argument::Reference {
            name: name.as_str().into(),
            expected: expected.clone(),
        })
    }

    fn resolve_locals(&mut self, scope: &mut Scope) -> Result<(), Error> {
        for name in scope.declarations.keys().cloned().collect::<Vec<_>>() {
            self.binding(&name, scope)?;
        }
        Ok(())
    }

    pub(super) fn ensure_predicate(&mut self, name: &str) -> Result<(), Error> {
        if self.predicates.contains_key(name) {
            return Ok(());
        }
        if !self.active.insert(name.into()) {
            return Err(Error(format!("cyclic predicate {name:?}")));
        }
        let predicate = match self.definitions.get(name).copied() {
            Some(syntax::Definition::DeclarePredicate(p)) => {
                let inputs = p.inputs().cloned().collect::<Vec<_>>();
                self.check_ports(&inputs)?;
                let mut seen = HashSet::new();
                for reason in p.errors() {
                    self.check_type(reason)?;
                    if !seen.insert(reason.spelling()) {
                        return Err(Error(format!("duplicate error {}", reason.spelling())));
                    }
                }
                Predicate {
                    inputs,
                    body: PredicateBody::Declared {
                        errors: p.errors().cloned().collect(),
                    },
                }
            }
            Some(syntax::Definition::Predicate(p)) => {
                let mut inputs = vec![];
                let mut scope = Scope::default();
                let mut test = None;
                for (i, m) in p.members.iter().enumerate() {
                    match m {
                        syntax::PredicateMember::Input(v) => {
                            if scope.contains(&v.name) {
                                return Err(Error(format!("duplicate binding {:?}", v.name)));
                            }
                            scope.types.insert(v.name.to_string(), v.ty.kind.clone());
                            inputs.push(v.clone());
                        }
                        syntax::PredicateMember::Binding(v) => scope.local(i, v)?,
                        syntax::PredicateMember::Predicate(v) => test = Some(v),
                    }
                }
                self.check_ports(&inputs)?;
                self.resolve_locals(&mut scope)?;
                let test = self.condition(
                    test.ok_or_else(|| Error("missing predicate condition".into()))?,
                    &mut scope,
                )?;
                Predicate {
                    inputs,
                    body: PredicateBody::Defined {
                        bindings: scope.bindings,
                        condition: test,
                    },
                }
            }
            _ => return Err(Error(format!("expected a predicate, got {name:?}"))),
        };
        self.predicates.insert(name.into(), predicate);
        self.active.remove(name);
        Ok(())
    }

    fn condition(
        &mut self,
        c: &syntax::PredicateExpression,
        scope: &mut Scope,
    ) -> Result<Condition, Error> {
        Ok(match c {
            syntax::PredicateExpression::Call {
                name,
                arguments: text_arguments,
            } => {
                self.ensure_predicate(name)?;
                let ports = self.predicates[name.as_str()].inputs.clone();
                let mut arguments = BTreeMap::new();
                for p in ports {
                    let text = text_arguments
                        .get(&p.name)
                        .ok_or_else(|| Error(format!("{name}: missing input {:?}", p.name)))?;
                    let argument = self.argument(text, &p.ty, scope)?;
                    arguments.insert(p.name.into_string(), argument);
                }
                if text_arguments.len() != arguments.len() {
                    return Err(Error(format!("{name}: unknown predicate input")));
                }
                Condition::Call {
                    name: name.to_string(),
                    arguments,
                }
            }
            syntax::PredicateExpression::All(ps) => Condition::All(
                ps.iter()
                    .map(|p| self.condition(p, scope))
                    .collect::<Result<_, _>>()?,
            ),
            syntax::PredicateExpression::Any(ps) => Condition::Any(
                ps.iter()
                    .map(|p| self.condition(p, scope))
                    .collect::<Result<_, _>>()?,
            ),
            syntax::PredicateExpression::Not(p) => {
                Condition::Not(Box::new(self.condition(p, scope)?))
            }
        })
    }

    fn scoped<T>(
        &mut self,
        outer: &mut Scope,
        f: impl FnOnce(&mut Self, &mut Scope) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut child = Scope {
            children: outer.children.clone(),
            outer: Some(Box::new(std::mem::take(outer))),
            ..Scope::default()
        };
        let result = f(self, &mut child);
        *outer = *child.outer.unwrap();
        result
    }

    fn branch(
        &mut self,
        b: &syntax::Branch,
        success: &TypeName,
        reason: Option<(&str, &TypeName)>,
        outer: &mut Scope,
    ) -> Result<Branch, Error> {
        self.scoped(outer, |this, scope| {
            if let Some((name, ty)) = reason {
                if scope.contains(name) {
                    return Err(Error(format!("shadowed catch binding {name:?}")));
                }
                scope.types.insert(name.into(), ty.clone());
            }
            let mut condition = None;
            for (i, member) in b.members.iter().enumerate() {
                match member {
                    syntax::BranchMember::Binding(v) => scope.local(i, v)?,
                    syntax::BranchMember::Predicate(c) => condition = Some(c),
                }
            }
            this.resolve_locals(scope)?;
            let condition = condition.map(|c| this.condition(c, scope)).transpose()?;
            let target = b.exit.parser_type(success);
            this.check_type(target)?;
            let result = this.argument(&b.exit.result, &plain(target), scope)?;
            Ok(Branch {
                condition,
                body: Block {
                    bindings: std::mem::take(&mut scope.bindings),
                    result,
                    throwing: b.exit.throw.is_some(),
                },
            })
        })
    }

    pub(super) fn select(
        &mut self,
        s: &syntax::Select,
        scope: &mut Scope,
    ) -> Result<Expression, Error> {
        self.check_type(&s.result_type)?;
        let branches = s
            .cases
            .iter()
            .map(|c| self.branch(c, &s.result_type, None, scope))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Expression {
            ty: s.result_type.clone(),
            kind: ExpressionKind::Select(branches),
        })
    }

    pub(super) fn attempt(
        &mut self,
        t: &syntax::Try,
        scope: &mut Scope,
    ) -> Result<Expression, Error> {
        self.check_type(&t.result_type)?;
        let body = self.scoped(scope, |this, local| {
            for (i, m) in t.members.iter().enumerate() {
                if let syntax::TryMember::Binding(v) = m {
                    local.local(i, v)?;
                }
            }
            this.resolve_locals(local)?;
            let result = this.argument(&t.result, &plain(&t.result_type), local)?;
            Ok(Block {
                bindings: std::mem::take(&mut local.bindings),
                result,
                throwing: false,
            })
        })?;
        let mut handlers = Vec::new();
        for m in &t.members {
            let (reason, catch) = match m {
                syntax::TryMember::Binding(_) => continue,
                syntax::TryMember::Unreachable { reason } => (reason, None),
                syntax::TryMember::Catch(c) => (&c.reason, Some(c)),
            };
            if let Some(reason) = reason {
                self.check_type(reason)?;
            }
            let action = if let Some(c) = catch {
                HandlerAction::Catch {
                    name: c.name.as_ref().map(ToString::to_string),
                    branch: self.branch(
                        &c.branch,
                        &t.result_type,
                        c.name.as_deref().zip(c.reason.as_ref()),
                        scope,
                    )?,
                }
            } else {
                HandlerAction::Unreachable
            };
            handlers.push(Handler {
                reason: reason.clone(),
                action,
            });
        }
        Ok(Expression {
            ty: t.result_type.clone(),
            kind: ExpressionKind::Try { body, handlers },
        })
    }
}
