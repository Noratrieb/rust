//! The lifetime resolution and elision code for late resolution

use RibKind::*;

use crate::late::LateResolutionVisitor;
use crate::late::LifetimeElisionCandidate;
use crate::late::MissingLifetime;
use crate::late::MissingLifetimeKind;
use crate::late::RibKind;
use crate::PathSource;
use crate::Res;
use crate::{path_names_to_string, BindingError, Finalize, LexicalScopeBinding};
use crate::{Module, ModuleOrUniformRoot, NameBinding, ParentScope, PathResult};
use crate::{ResolutionError, Resolver, Segment, UseError};
use rustc_ast::ptr::P;
use rustc_ast::visit::{self, AssocCtxt, BoundKind, FnCtxt, FnKind, Visitor};
use rustc_ast::*;
use rustc_data_structures::fx::{FxHashMap, FxHashSet, FxIndexMap};
use rustc_errors::DiagnosticId;
use rustc_hir::def::Namespace::{self, *};
use rustc_hir::def::{self, CtorKind, DefKind, LifetimeRes, PartialRes, PerNS};
use rustc_hir::def_id::{DefId, LocalDefId, CRATE_DEF_ID, LOCAL_CRATE};
use rustc_hir::{BindingAnnotation, PrimTy, TraitCandidate};
use rustc_middle::middle::resolve_lifetime::Set1;
use rustc_middle::ty::DefIdTree;
use rustc_middle::{bug, span_bug};
use rustc_session::lint;
use rustc_span::source_map::{respan, Spanned};
use rustc_span::symbol::{kw, sym, Ident, Symbol};
use rustc_span::{BytePos, Span};
use smallvec::{smallvec, SmallVec};
use std::collections::{hash_map::Entry, BTreeSet};
use std::mem::{replace, take};

#[derive(Copy, Clone, Debug)]
pub(super) enum LifetimeRibKind {
    // -- Ribs introducing named lifetimes
    //
    /// This rib declares generic parameters.
    /// Only for this kind the `LifetimeRib::bindings` field can be non-empty.
    Generics { binder: NodeId, span: Span, kind: LifetimeBinderKind },

    // -- Ribs introducing unnamed lifetimes
    //
    /// Create a new anonymous lifetime parameter and reference it.
    ///
    /// If `report_in_path`, report an error when encountering lifetime elision in a path:
    /// ```compile_fail
    /// struct Foo<'a> { x: &'a () }
    /// async fn foo(x: Foo) {}
    /// ```
    ///
    /// Note: the error should not trigger when the elided lifetime is in a pattern or
    /// expression-position path:
    /// ```
    /// struct Foo<'a> { x: &'a () }
    /// async fn foo(Foo { x: _ }: Foo<'_>) {}
    /// ```
    AnonymousCreateParameter { binder: NodeId, report_in_path: bool },

    /// Replace all anonymous lifetimes by provided lifetime.
    Elided(LifetimeRes),

    // -- Barrier ribs that stop lifetime lookup, or continue it but produce an error later.
    //
    /// Give a hard error when either `&` or `'_` is written. Used to
    /// rule out things like `where T: Foo<'_>`. Does not imply an
    /// error on default object bounds (e.g., `Box<dyn Foo>`).
    AnonymousReportError,

    /// Signal we cannot find which should be the anonymous lifetime.
    ElisionFailure,

    /// FIXME(const_generics): This patches over an ICE caused by non-'static lifetimes in const
    /// generics. We are disallowing this until we can decide on how we want to handle non-'static
    /// lifetimes in const generics. See issue #74052 for discussion.
    ConstGeneric,

    /// Non-static lifetimes are prohibited in anonymous constants under `min_const_generics`.
    /// This function will emit an error if `generic_const_exprs` is not enabled, the body
    /// identified by `body_id` is an anonymous constant and `lifetime_ref` is non-static.
    AnonConst,

    /// This rib acts as a barrier to forbid reference to lifetimes of a parent item.
    Item,
}

#[derive(Copy, Clone, Debug)]
pub(super) enum LifetimeBinderKind {
    BareFnType,
    PolyTrait,
    WhereBound,
    Item,
    Function,
    Closure,
    ImplBlock,
}

impl LifetimeBinderKind {
    pub(super) fn descr(self) -> &'static str {
        use LifetimeBinderKind::*;
        match self {
            BareFnType => "type",
            PolyTrait => "bound",
            WhereBound => "bound",
            Item => "item",
            ImplBlock => "impl block",
            Function => "function",
            Closure => "closure",
        }
    }
}

#[derive(Debug)]
pub(super) struct LifetimeRib {
    pub kind: LifetimeRibKind,
    // We need to preserve insertion order for async fns.
    pub bindings: FxIndexMap<Ident, (NodeId, LifetimeRes)>,
}

impl LifetimeRib {
    pub fn new(kind: LifetimeRibKind) -> LifetimeRib {
        LifetimeRib { bindings: Default::default(), kind }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LifetimeUseSet {
    One { use_span: Span, use_ctxt: visit::LifetimeCtxt },
    Many,
}

impl<'a: 'ast, 'b, 'ast> LateResolutionVisitor<'a, 'b, 'ast> {
    #[instrument(level = "debug", skip(self, work))]
    pub(super) fn with_lifetime_rib<T>(
        &mut self,
        kind: LifetimeRibKind,
        work: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.lifetime_ribs.push(LifetimeRib::new(kind));
        let outer_elision_candidates = self.lifetime_elision_candidates.take();
        let ret = work(self);
        self.lifetime_elision_candidates = outer_elision_candidates;
        self.lifetime_ribs.pop();
        ret
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn resolve_lifetime(
        &mut self,
        lifetime: &'ast Lifetime,
        use_ctxt: visit::LifetimeCtxt,
    ) {
        let ident = lifetime.ident;

        if ident.name == kw::StaticLifetime {
            self.record_lifetime_res(
                lifetime.id,
                LifetimeRes::Static,
                LifetimeElisionCandidate::Named,
            );
            return;
        }

        if ident.name == kw::UnderscoreLifetime {
            return self.resolve_anonymous_lifetime(lifetime, false);
        }

        let mut lifetime_rib_iter = self.lifetime_ribs.iter().rev();
        while let Some(rib) = lifetime_rib_iter.next() {
            let normalized_ident = ident.normalize_to_macros_2_0();
            if let Some(&(_, res)) = rib.bindings.get(&normalized_ident) {
                self.record_lifetime_res(lifetime.id, res, LifetimeElisionCandidate::Named);

                if let LifetimeRes::Param { param, .. } = res {
                    match self.lifetime_uses.entry(param) {
                        Entry::Vacant(v) => {
                            debug!("First use of {:?} at {:?}", res, ident.span);
                            let use_set = self
                                .lifetime_ribs
                                .iter()
                                .rev()
                                .find_map(|rib| match rib.kind {
                                    // Do not suggest eliding a lifetime where an anonymous
                                    // lifetime would be illegal.
                                    LifetimeRibKind::Item
                                    | LifetimeRibKind::AnonymousReportError
                                    | LifetimeRibKind::ElisionFailure => Some(LifetimeUseSet::Many),
                                    // An anonymous lifetime is legal here, go ahead.
                                    LifetimeRibKind::AnonymousCreateParameter { .. } => {
                                        Some(LifetimeUseSet::One { use_span: ident.span, use_ctxt })
                                    }
                                    // Only report if eliding the lifetime would have the same
                                    // semantics.
                                    LifetimeRibKind::Elided(r) => Some(if res == r {
                                        LifetimeUseSet::One { use_span: ident.span, use_ctxt }
                                    } else {
                                        LifetimeUseSet::Many
                                    }),
                                    LifetimeRibKind::Generics { .. } => None,
                                    LifetimeRibKind::ConstGeneric | LifetimeRibKind::AnonConst => {
                                        span_bug!(ident.span, "unexpected rib kind: {:?}", rib.kind)
                                    }
                                })
                                .unwrap_or(LifetimeUseSet::Many);
                            debug!(?use_ctxt, ?use_set);
                            v.insert(use_set);
                        }
                        Entry::Occupied(mut o) => {
                            debug!("Many uses of {:?} at {:?}", res, ident.span);
                            *o.get_mut() = LifetimeUseSet::Many;
                        }
                    }
                }
                return;
            }

            match rib.kind {
                LifetimeRibKind::Item => break,
                LifetimeRibKind::ConstGeneric => {
                    self.emit_non_static_lt_in_const_generic_error(lifetime);
                    self.record_lifetime_res(
                        lifetime.id,
                        LifetimeRes::Error,
                        LifetimeElisionCandidate::Ignore,
                    );
                    return;
                }
                LifetimeRibKind::AnonConst => {
                    self.maybe_emit_forbidden_non_static_lifetime_error(lifetime);
                    self.record_lifetime_res(
                        lifetime.id,
                        LifetimeRes::Error,
                        LifetimeElisionCandidate::Ignore,
                    );
                    return;
                }
                LifetimeRibKind::AnonymousCreateParameter { .. }
                | LifetimeRibKind::Elided(_)
                | LifetimeRibKind::Generics { .. }
                | LifetimeRibKind::ElisionFailure
                | LifetimeRibKind::AnonymousReportError => {}
            }
        }

        let mut outer_res = None;
        for rib in lifetime_rib_iter {
            let normalized_ident = ident.normalize_to_macros_2_0();
            if let Some((&outer, _)) = rib.bindings.get_key_value(&normalized_ident) {
                outer_res = Some(outer);
                break;
            }
        }

        self.emit_undeclared_lifetime_error(lifetime, outer_res);
        self.record_lifetime_res(lifetime.id, LifetimeRes::Error, LifetimeElisionCandidate::Named);
    }

    /// Resolve inside function parameters and parameter types.
    /// Returns the lifetime for elision in fn return type,
    /// or diagnostic information in case of elision failure.
    pub(super) fn resolve_fn_params(
        &mut self,
        has_self: bool,
        inputs: impl Iterator<Item = (Option<&'ast Pat>, &'ast Ty)>,
    ) -> Result<LifetimeRes, (Vec<MissingLifetime>, Vec<ElisionFnParameter>)> {
        let outer_candidates =
            replace(&mut self.lifetime_elision_candidates, Some(Default::default()));

        let mut elision_lifetime = None;
        let mut lifetime_count = 0;
        let mut parameter_info = Vec::new();

        let mut bindings = smallvec![(PatBoundCtx::Product, Default::default())];
        for (index, (pat, ty)) in inputs.enumerate() {
            debug!(?pat, ?ty);
            self.with_lifetime_rib(LifetimeRibKind::Elided(LifetimeRes::Infer), |this| {
                if let Some(pat) = pat {
                    this.resolve_pattern(pat, PatternSource::FnParam, &mut bindings);
                }
            });
            self.visit_ty(ty);

            if let Some(ref candidates) = self.lifetime_elision_candidates {
                let new_count = candidates.len();
                let local_count = new_count - lifetime_count;
                if local_count != 0 {
                    parameter_info.push(ElisionFnParameter {
                        index,
                        ident: if let Some(pat) = pat && let PatKind::Ident(_, ident, _) = pat.kind {
                            Some(ident)
                        } else {
                            None
                        },
                        lifetime_count: local_count,
                        span: ty.span,
                    });
                }
                lifetime_count = new_count;
            }

            // Handle `self` specially.
            if index == 0 && has_self {
                let self_lifetime = self.find_lifetime_for_self(ty);
                if let Set1::One(lifetime) = self_lifetime {
                    elision_lifetime = Some(lifetime);
                    self.lifetime_elision_candidates = None;
                } else {
                    self.lifetime_elision_candidates = Some(Default::default());
                    lifetime_count = 0;
                }
            }
            debug!("(resolving function / closure) recorded parameter");
        }

        let all_candidates = replace(&mut self.lifetime_elision_candidates, outer_candidates);
        debug!(?all_candidates);

        if let Some(res) = elision_lifetime {
            return Ok(res);
        }

        // We do not have a `self` candidate, look at the full list.
        let all_candidates = all_candidates.unwrap();
        if all_candidates.len() == 1 {
            Ok(*all_candidates.first().unwrap().0)
        } else {
            let all_candidates = all_candidates
                .into_iter()
                .filter_map(|(_, candidate)| match candidate {
                    LifetimeElisionCandidate::Ignore | LifetimeElisionCandidate::Named => None,
                    LifetimeElisionCandidate::Missing(missing) => Some(missing),
                })
                .collect();
            Err((all_candidates, parameter_info))
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn resolve_anonymous_lifetime(&mut self, lifetime: &Lifetime, elided: bool) {
        debug_assert_eq!(lifetime.ident.name, kw::UnderscoreLifetime);

        let missing_lifetime = MissingLifetime {
            id: lifetime.id,
            span: lifetime.ident.span,
            kind: if elided {
                MissingLifetimeKind::Ampersand
            } else {
                MissingLifetimeKind::Underscore
            },
            count: 1,
        };
        let elision_candidate = LifetimeElisionCandidate::Missing(missing_lifetime);
        for rib in self.lifetime_ribs.iter().rev() {
            debug!(?rib.kind);
            match rib.kind {
                LifetimeRibKind::AnonymousCreateParameter { binder, .. } => {
                    let res = self.create_fresh_lifetime(lifetime.id, lifetime.ident, binder);
                    self.record_lifetime_res(lifetime.id, res, elision_candidate);
                    return;
                }
                LifetimeRibKind::AnonymousReportError => {
                    let (msg, note) = if elided {
                        (
                            "`&` without an explicit lifetime name cannot be used here",
                            "explicit lifetime name needed here",
                        )
                    } else {
                        ("`'_` cannot be used here", "`'_` is a reserved lifetime name")
                    };
                    rustc_errors::struct_span_err!(
                        self.r.session,
                        lifetime.ident.span,
                        E0637,
                        "{}",
                        msg,
                    )
                    .span_label(lifetime.ident.span, note)
                    .emit();

                    self.record_lifetime_res(lifetime.id, LifetimeRes::Error, elision_candidate);
                    return;
                }
                LifetimeRibKind::Elided(res) => {
                    self.record_lifetime_res(lifetime.id, res, elision_candidate);
                    return;
                }
                LifetimeRibKind::ElisionFailure => {
                    self.diagnostic_metadata.current_elision_failures.push(missing_lifetime);
                    self.record_lifetime_res(lifetime.id, LifetimeRes::Error, elision_candidate);
                    return;
                }
                LifetimeRibKind::Item => break,
                LifetimeRibKind::Generics { .. } | LifetimeRibKind::ConstGeneric => {}
                LifetimeRibKind::AnonConst => {
                    // There is always an `Elided(LifetimeRes::Static)` inside an `AnonConst`.
                    span_bug!(lifetime.ident.span, "unexpected rib kind: {:?}", rib.kind)
                }
            }
        }
        self.record_lifetime_res(lifetime.id, LifetimeRes::Error, elision_candidate);
        self.report_missing_lifetime_specifiers(vec![missing_lifetime], None);
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn resolve_elided_lifetime(&mut self, anchor_id: NodeId, span: Span) {
        let id = self.r.next_node_id();
        let lt = Lifetime { id, ident: Ident::new(kw::UnderscoreLifetime, span) };

        self.record_lifetime_res(
            anchor_id,
            LifetimeRes::ElidedAnchor { start: id, end: NodeId::from_u32(id.as_u32() + 1) },
            LifetimeElisionCandidate::Ignore,
        );
        self.resolve_anonymous_lifetime(&lt, true);
    }

    #[instrument(level = "debug", skip(self))]
    fn create_fresh_lifetime(&mut self, id: NodeId, ident: Ident, binder: NodeId) -> LifetimeRes {
        debug_assert_eq!(ident.name, kw::UnderscoreLifetime);
        debug!(?ident.span);

        // Leave the responsibility to create the `LocalDefId` to lowering.
        let param = self.r.next_node_id();
        let res = LifetimeRes::Fresh { param, binder };

        // Record the created lifetime parameter so lowering can pick it up and add it to HIR.
        self.r
            .extra_lifetime_params_map
            .entry(binder)
            .or_insert_with(Vec::new)
            .push((ident, param, res));
        res
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn resolve_elided_lifetimes_in_path(
        &mut self,
        path_id: NodeId,
        partial_res: PartialRes,
        path: &[Segment],
        source: PathSource<'_>,
        path_span: Span,
    ) {
        let proj_start = path.len() - partial_res.unresolved_segments();
        for (i, segment) in path.iter().enumerate() {
            if segment.has_lifetime_args {
                continue;
            }
            let Some(segment_id) = segment.id else {
                continue;
            };

            // Figure out if this is a type/trait segment,
            // which may need lifetime elision performed.
            let type_def_id = match partial_res.base_res() {
                Res::Def(DefKind::AssocTy, def_id) if i + 2 == proj_start => self.r.parent(def_id),
                Res::Def(DefKind::Variant, def_id) if i + 1 == proj_start => self.r.parent(def_id),
                Res::Def(DefKind::Struct, def_id)
                | Res::Def(DefKind::Union, def_id)
                | Res::Def(DefKind::Enum, def_id)
                | Res::Def(DefKind::TyAlias, def_id)
                | Res::Def(DefKind::Trait, def_id)
                    if i + 1 == proj_start =>
                {
                    def_id
                }
                _ => continue,
            };

            let expected_lifetimes = self.r.item_generics_num_lifetimes(type_def_id);
            if expected_lifetimes == 0 {
                continue;
            }

            let node_ids = self.r.next_node_ids(expected_lifetimes);
            self.record_lifetime_res(
                segment_id,
                LifetimeRes::ElidedAnchor { start: node_ids.start, end: node_ids.end },
                LifetimeElisionCandidate::Ignore,
            );

            let inferred = match source {
                PathSource::Trait(..) | PathSource::TraitItem(..) | PathSource::Type => false,
                PathSource::Expr(..)
                | PathSource::Pat
                | PathSource::Struct
                | PathSource::TupleStruct(..) => true,
            };
            if inferred {
                // Do not create a parameter for patterns and expressions: type checking can infer
                // the appropriate lifetime for us.
                for id in node_ids {
                    self.record_lifetime_res(
                        id,
                        LifetimeRes::Infer,
                        LifetimeElisionCandidate::Named,
                    );
                }
                continue;
            }

            let elided_lifetime_span = if segment.has_generic_args {
                // If there are brackets, but not generic arguments, then use the opening bracket
                segment.args_span.with_hi(segment.args_span.lo() + BytePos(1))
            } else {
                // If there are no brackets, use the identifier span.
                // HACK: we use find_ancestor_inside to properly suggest elided spans in paths
                // originating from macros, since the segment's span might be from a macro arg.
                segment.ident.span.find_ancestor_inside(path_span).unwrap_or(path_span)
            };
            let ident = Ident::new(kw::UnderscoreLifetime, elided_lifetime_span);

            let missing_lifetime = MissingLifetime {
                id: node_ids.start,
                span: elided_lifetime_span,
                kind: if segment.has_generic_args {
                    MissingLifetimeKind::Comma
                } else {
                    MissingLifetimeKind::Brackets
                },
                count: expected_lifetimes,
            };
            let mut should_lint = true;
            for rib in self.lifetime_ribs.iter().rev() {
                match rib.kind {
                    // In create-parameter mode we error here because we don't want to support
                    // deprecated impl elision in new features like impl elision and `async fn`,
                    // both of which work using the `CreateParameter` mode:
                    //
                    //     impl Foo for std::cell::Ref<u32> // note lack of '_
                    //     async fn foo(_: std::cell::Ref<u32>) { ... }
                    LifetimeRibKind::AnonymousCreateParameter { report_in_path: true, .. } => {
                        let sess = self.r.session;
                        let mut err = rustc_errors::struct_span_err!(
                            sess,
                            path_span,
                            E0726,
                            "implicit elided lifetime not allowed here"
                        );
                        rustc_errors::add_elided_lifetime_in_path_suggestion(
                            sess.source_map(),
                            &mut err,
                            expected_lifetimes,
                            path_span,
                            !segment.has_generic_args,
                            elided_lifetime_span,
                        );
                        err.note("assuming a `'static` lifetime...");
                        err.emit();
                        should_lint = false;

                        for id in node_ids {
                            self.record_lifetime_res(
                                id,
                                LifetimeRes::Error,
                                LifetimeElisionCandidate::Named,
                            );
                        }
                        break;
                    }
                    // Do not create a parameter for patterns and expressions.
                    LifetimeRibKind::AnonymousCreateParameter { binder, .. } => {
                        // Group all suggestions into the first record.
                        let mut candidate = LifetimeElisionCandidate::Missing(missing_lifetime);
                        for id in node_ids {
                            let res = self.create_fresh_lifetime(id, ident, binder);
                            self.record_lifetime_res(
                                id,
                                res,
                                replace(&mut candidate, LifetimeElisionCandidate::Named),
                            );
                        }
                        break;
                    }
                    LifetimeRibKind::Elided(res) => {
                        let mut candidate = LifetimeElisionCandidate::Missing(missing_lifetime);
                        for id in node_ids {
                            self.record_lifetime_res(
                                id,
                                res,
                                replace(&mut candidate, LifetimeElisionCandidate::Ignore),
                            );
                        }
                        break;
                    }
                    LifetimeRibKind::ElisionFailure => {
                        self.diagnostic_metadata.current_elision_failures.push(missing_lifetime);
                        for id in node_ids {
                            self.record_lifetime_res(
                                id,
                                LifetimeRes::Error,
                                LifetimeElisionCandidate::Ignore,
                            );
                        }
                        break;
                    }
                    // `LifetimeRes::Error`, which would usually be used in the case of
                    // `ReportError`, is unsuitable here, as we don't emit an error yet.  Instead,
                    // we simply resolve to an implicit lifetime, which will be checked later, at
                    // which point a suitable error will be emitted.
                    LifetimeRibKind::AnonymousReportError | LifetimeRibKind::Item => {
                        for id in node_ids {
                            self.record_lifetime_res(
                                id,
                                LifetimeRes::Error,
                                LifetimeElisionCandidate::Ignore,
                            );
                        }
                        self.report_missing_lifetime_specifiers(vec![missing_lifetime], None);
                        break;
                    }
                    LifetimeRibKind::Generics { .. } | LifetimeRibKind::ConstGeneric => {}
                    LifetimeRibKind::AnonConst => {
                        // There is always an `Elided(LifetimeRes::Static)` inside an `AnonConst`.
                        span_bug!(elided_lifetime_span, "unexpected rib kind: {:?}", rib.kind)
                    }
                }
            }

            if should_lint {
                self.r.lint_buffer.buffer_lint_with_diagnostic(
                    lint::builtin::ELIDED_LIFETIMES_IN_PATHS,
                    segment_id,
                    elided_lifetime_span,
                    "hidden lifetime parameters in types are deprecated",
                    lint::BuiltinLintDiagnostics::ElidedLifetimesInPaths(
                        expected_lifetimes,
                        path_span,
                        !segment.has_generic_args,
                        elided_lifetime_span,
                    ),
                );
            }
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn record_lifetime_res(
        &mut self,
        id: NodeId,
        res: LifetimeRes,
        candidate: LifetimeElisionCandidate,
    ) {
        if let Some(prev_res) = self.r.lifetimes_res_map.insert(id, res) {
            panic!(
                "lifetime {:?} resolved multiple times ({:?} before, {:?} now)",
                id, prev_res, res
            )
        }
        match res {
            LifetimeRes::Param { .. } | LifetimeRes::Fresh { .. } | LifetimeRes::Static => {
                if let Some(ref mut candidates) = self.lifetime_elision_candidates {
                    candidates.insert(res, candidate);
                }
            }
            LifetimeRes::Infer | LifetimeRes::Error | LifetimeRes::ElidedAnchor { .. } => {}
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn record_lifetime_param(&mut self, id: NodeId, res: LifetimeRes) {
        if let Some(prev_res) = self.r.lifetimes_res_map.insert(id, res) {
            panic!(
                "lifetime parameter {:?} resolved multiple times ({:?} before, {:?} now)",
                id, prev_res, res
            )
        }
    }

    /// Construct the list of in-scope lifetime parameters for async lowering.
    /// We include all lifetime parameters, either named or "Fresh".
    /// The order of those parameters does not matter, as long as it is
    /// deterministic.
    pub(super) fn record_lifetime_params_for_async(
        &mut self,
        fn_id: NodeId,
        async_node_id: Option<(NodeId, Span)>,
    ) {
        if let Some((async_node_id, span)) = async_node_id {
            let mut extra_lifetime_params =
                self.r.extra_lifetime_params_map.get(&fn_id).cloned().unwrap_or_default();
            for rib in self.lifetime_ribs.iter().rev() {
                extra_lifetime_params.extend(
                    rib.bindings.iter().map(|(&ident, &(node_id, res))| (ident, node_id, res)),
                );
                match rib.kind {
                    LifetimeRibKind::Item => break,
                    LifetimeRibKind::AnonymousCreateParameter { binder, .. } => {
                        if let Some(earlier_fresh) = self.r.extra_lifetime_params_map.get(&binder) {
                            extra_lifetime_params.extend(earlier_fresh);
                        }
                    }
                    LifetimeRibKind::Generics { .. } => {}
                    _ => {
                        // We are in a function definition. We should only find `Generics`
                        // and `AnonymousCreateParameter` inside the innermost `Item`.
                        span_bug!(span, "unexpected rib kind: {:?}", rib.kind)
                    }
                }
            }
            self.r.extra_lifetime_params_map.insert(async_node_id, extra_lifetime_params);
        }
    }

    /// List all the lifetimes that appear in the provided type.
    pub(super) fn find_lifetime_for_self(&self, ty: &'ast Ty) -> Set1<LifetimeRes> {
        struct SelfVisitor<'r, 'a> {
            r: &'r Resolver<'a>,
            impl_self: Option<Res>,
            lifetime: Set1<LifetimeRes>,
        }

        impl SelfVisitor<'_, '_> {
            // Look for `self: &'a Self` - also desugared from `&'a self`,
            // and if that matches, use it for elision and return early.
            fn is_self_ty(&self, ty: &Ty) -> bool {
                match ty.kind {
                    TyKind::ImplicitSelf => true,
                    TyKind::Path(None, _) => {
                        let path_res = self.r.partial_res_map[&ty.id].expect_full_res();
                        if let Res::SelfTyParam { .. } | Res::SelfTyAlias { .. } = path_res {
                            return true;
                        }
                        Some(path_res) == self.impl_self
                    }
                    _ => false,
                }
            }
        }

        impl<'a> Visitor<'a> for SelfVisitor<'_, '_> {
            fn visit_ty(&mut self, ty: &'a Ty) {
                trace!("SelfVisitor considering ty={:?}", ty);
                if let TyKind::Rptr(lt, ref mt) = ty.kind && self.is_self_ty(&mt.ty) {
                        let lt_id = if let Some(lt) = lt {
                            lt.id
                        } else {
                            let res = self.r.lifetimes_res_map[&ty.id];
                            let LifetimeRes::ElidedAnchor { start, .. } = res else { bug!() };
                            start
                        };
                        let lt_res = self.r.lifetimes_res_map[&lt_id];
                        trace!("SelfVisitor inserting res={:?}", lt_res);
                        self.lifetime.insert(lt_res);
                    }
                visit::walk_ty(self, ty)
            }
        }

        let impl_self = self
            .diagnostic_metadata
            .current_self_type
            .as_ref()
            .and_then(|ty| {
                if let TyKind::Path(None, _) = ty.kind {
                    self.r.partial_res_map.get(&ty.id)
                } else {
                    None
                }
            })
            .and_then(|res| res.full_res())
            .filter(|res| {
                // Permit the types that unambiguously always
                // result in the same type constructor being used
                // (it can't differ between `Self` and `self`).
                matches!(
                    res,
                    Res::Def(DefKind::Struct | DefKind::Union | DefKind::Enum, _,) | Res::PrimTy(_)
                )
            });
        let mut visitor = SelfVisitor { r: self.r, impl_self, lifetime: Set1::Empty };
        visitor.visit_ty(ty);
        trace!("SelfVisitor found={:?}", visitor.lifetime);
        visitor.lifetime
    }
}

pub fn count_lifetimes(resolver: &mut Resolver<'_>, krate: &Crate) {
    visit::walk_crate(&mut LifetimeCountVisitor { r: self }, krate);
}

pub(super) struct LifetimeCountVisitor<'a, 'b> {
    r: &'b mut Resolver<'a>,
}

/// Walks the whole crate in DFS order, visiting each item, counting the declared number of
/// lifetime generic parameters.
impl<'ast> Visitor<'ast> for LifetimeCountVisitor<'_, '_> {
    fn visit_item(&mut self, item: &'ast Item) {
        match &item.kind {
            ItemKind::TyAlias(box TyAlias { ref generics, .. })
            | ItemKind::Fn(box Fn { ref generics, .. })
            | ItemKind::Enum(_, ref generics)
            | ItemKind::Struct(_, ref generics)
            | ItemKind::Union(_, ref generics)
            | ItemKind::Impl(box Impl { ref generics, .. })
            | ItemKind::Trait(box Trait { ref generics, .. })
            | ItemKind::TraitAlias(ref generics, _) => {
                let def_id = self.r.local_def_id(item.id);
                let count = generics
                    .params
                    .iter()
                    .filter(|param| matches!(param.kind, ast::GenericParamKind::Lifetime { .. }))
                    .count();
                self.r.item_generics_num_lifetimes.insert(def_id, count);
            }

            ItemKind::Mod(..)
            | ItemKind::ForeignMod(..)
            | ItemKind::Static(..)
            | ItemKind::Const(..)
            | ItemKind::Use(..)
            | ItemKind::ExternCrate(..)
            | ItemKind::MacroDef(..)
            | ItemKind::GlobalAsm(..)
            | ItemKind::MacCall(..) => {}
        }
        visit::walk_item(self, item)
    }
}
