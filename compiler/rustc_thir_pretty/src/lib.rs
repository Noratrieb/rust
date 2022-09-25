#![cfg_attr(bootstrap, feature(let_else))]

use rustc_ast::ast;
use rustc_ast_pretty::pprust::Comments;
use rustc_ast_pretty::{
    pp::{self, Breaks},
    pprust::PrintState,
};
use rustc_hir as hir;
use rustc_middle::{
    thir::Thir,
    ty::{self, TyCtxt},
};
use rustc_span::symbol::IdentPrinter;
use rustc_span::{source_map::SourceMap, symbol::Ident, FileName, Symbol};
use rustc_target::spec::abi::Abi;

pub fn write_thir_pretty(
    tcx: TyCtxt<'_>,
    sm: &SourceMap,
    filename: FileName,
    input: String,
) -> String {
    let mut out = String::new();

    for did in tcx.hir().body_owners() {
        match tcx.thir_body_clone(ty::WithOptConstParam::unknown(did)) {
            Ok(thir) => {
                let mut s = State::new_from_input(sm, filename.clone(), input.clone());

                let hir::Node::Item(item) = tcx.hir().get_by_def_id(did) else { continue };
                s.print_fn(&thir, &item);

                out.push_str(&s.s.eof());
                out.push_str("\n\n");
            }
            Err(_) => {
                out.push_str("<error getting THIR>");
            }
        }
    }

    out
}

pub struct State<'a> {
    pub s: pp::Printer,
    comments: Option<Comments<'a>>,
}

impl std::ops::Deref for State<'_> {
    type Target = pp::Printer;
    fn deref(&self) -> &Self::Target {
        &self.s
    }
}

impl std::ops::DerefMut for State<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.s
    }
}

impl<'a> PrintState<'a> for State<'a> {
    fn comments(&mut self) -> &mut Option<Comments<'a>> {
        &mut self.comments
    }

    fn print_ident(&mut self, ident: Ident) {
        self.word(IdentPrinter::for_ast_ident(ident, ident.is_raw_guess()).to_string());
    }

    fn print_generic_args(&mut self, _: &ast::GenericArgs, _colons_before_params: bool) {
        panic!("AST generic args printed by THIR pretty-printer");
    }
}

pub const INDENT_UNIT: isize = 4;

impl<'a> State<'a> {
    pub fn new_from_input(sm: &'a SourceMap, filename: FileName, input: String) -> State<'a> {
        State { s: pp::Printer::new(), comments: Some(Comments::new(sm, filename, input)) }
    }
}

impl State<'_> {
    pub fn print_fn_header_info(&mut self, header: hir::FnHeader) {
        match header.constness {
            hir::Constness::NotConst => {}
            hir::Constness::Const => self.word_nbsp("const"),
        }

        match header.asyncness {
            hir::IsAsync::NotAsync => {}
            hir::IsAsync::Async => self.word_nbsp("async"),
        }

        self.print_unsafety(header.unsafety);

        if header.abi != Abi::Rust {
            self.word_nbsp("extern");
            self.word_nbsp(header.abi.to_string());
        }

        self.word("fn")
    }

    pub fn print_unsafety(&mut self, s: hir::Unsafety) {
        match s {
            hir::Unsafety::Normal => {}
            hir::Unsafety::Unsafe => self.word_nbsp("unsafe"),
        }
    }

    pub fn print_name(&mut self, name: Symbol) {
        self.print_ident(Ident::with_dummy_span(name))
    }

    fn print_fn(&mut self, thir: &Thir<'_>, hir: &hir::Item<'_>) {
        let hir::ItemKind::Fn(sig, _, _) = &hir.kind else { return };

        self.print_fn_header_info(sig.header);

        self.nbsp();
        self.print_name(hir.ident.name);

        self.word("<>");

        self.popen();
        self.pclose();

        self.print_fn_output(&sig.decl);

        for stmt in &thir.stmts {}
    }

    pub fn print_fn_output(&mut self, decl: &hir::FnDecl<'_>) {
        if let hir::FnRetTy::DefaultReturn(..) = decl.output {
            return;
        }

        self.space_if_not_bol();
        self.ibox(INDENT_UNIT);
        self.word_space("->");
        match decl.output {
            hir::FnRetTy::DefaultReturn(..) => unreachable!(),
            hir::FnRetTy::Return(ty) => self.word("_"),
        }
        self.end();

        if let hir::FnRetTy::Return(output) = decl.output {
            self.maybe_print_comment(output.span.lo());
        }
    }
}
