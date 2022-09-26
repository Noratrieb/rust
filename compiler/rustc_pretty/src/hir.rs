use crate::pp::Printer as PpPrinter;
use rustc_ast::ast;
use rustc_hir as hir;
use rustc_span::{source_map::SourceMap, FileName, Symbol};

type Printer<'a> = PpPrinter<HirExtraPrinter<'a>>;

/// Requires you to pass an input filename and reader so that
/// it can scan the input text for comments to copy forward.
pub fn print_crate<'a>(
    sm: &'a SourceMap,
    krate: &hir::Mod<'_>,
    filename: FileName,
    input: String,
    attrs: &'a dyn Fn(hir::HirId) -> &'a [ast::Attribute],
    ann: &'a dyn PpAnn,
) -> String {
    let mut p = Printer::new(HirExtraPrinter { attrs, ann });
    // When printing the AST, we sometimes need to inject `#[no_std]` here.
    // Since you can't compile the HIR, it's not necessary.

    p.print_file(krate, (*attrs)(hir::CRATE_HIR_ID));
    p.eof()
}

pub trait PpAnn {
    fn nested(&self, _state: &mut State<'_>, _nested: Nested) {}
    fn pre(&self, _state: &mut State<'_>, _node: AnnNode<'_>) {}
    fn post(&self, _state: &mut State<'_>, _node: AnnNode<'_>) {}
}

pub enum AnnNode<'a> {
    Name(&'a Symbol),
    Block(&'a hir::Block<'a>),
    Item(&'a hir::Item<'a>),
    SubItem(hir::HirId),
    Expr(&'a hir::Expr<'a>),
    Pat(&'a hir::Pat<'a>),
    Arm(&'a hir::Arm<'a>),
}

pub enum Nested {
    Item(hir::ItemId),
    TraitItem(hir::TraitItemId),
    ImplItem(hir::ImplItemId),
    ForeignItem(hir::ForeignItemId),
    Body(hir::BodyId),
    BodyParamPat(hir::BodyId, usize),
}

pub struct NoAnn;
impl PpAnn for NoAnn {}
pub const NO_ANN: &dyn PpAnn = &NoAnn;

/// Identical to the `PpAnn` implementation for `hir::Crate`,
/// except it avoids creating a dependency on the whole crate.
impl PpAnn for &dyn rustc_hir::intravisit::Map<'_> {
    fn nested(&self, state: &mut State<'_>, nested: Nested) {
        match nested {
            Nested::Item(id) => state.print_item(self.item(id)),
            Nested::TraitItem(id) => state.print_trait_item(self.trait_item(id)),
            Nested::ImplItem(id) => state.print_impl_item(self.impl_item(id)),
            Nested::ForeignItem(id) => state.print_foreign_item(self.foreign_item(id)),
            Nested::Body(id) => state.print_expr(&self.body(id).value),
            Nested::BodyParamPat(id, i) => state.print_pat(self.body(id).params[i].pat),
        }
    }
}

struct HirExtraPrinter<'a> {
    attrs: &'a dyn Fn(hir::HirId) -> &'a [ast::Attribute],
    ann: &'a (dyn PpAnn + 'a),
}

impl Printer<'_> {
    fn print_file(&mut self, krate: &hir::Mod<'_>, attrs: &[ast::Attribute]) {
        self.cbox(0);

        self.ast_print_inner_attributes(attrs);

        for &item_id in krate.item_ids {
            self.print_item(todo!());
        }

        self.end();
    }

    fn print_mod(&mut self, krate: &hir::Mod<'_>, attrs: &[ast::Attribute]) {
        self.ast_print_inner_attributes(attrs);
    }

    pub fn print_item(&mut self, item: &hir::Item<'_>) {}
}
