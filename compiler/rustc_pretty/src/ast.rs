use crate::pp::Printer as PpPrinter;
use rustc_ast::ast;

type Printer = PpPrinter<()>;

// Public methods also used by other pretty-printers
impl<T> Printer<T> {
    pub fn ast_print_inner_attributes(&mut self, attrs: &[ast::Attribute]) {
        for attr in attrs {
            if attr.style == ast::AttrStyle::Inner {
                self.print_attr(attr);
            }
        }
    }
}

impl Printer {
    fn print_attr(&mut self, attr: &ast::Attribute) {
        // FIXME: Special case doc here?
        self.word(match attr.style {
            ast::AttrStyle::Outer => "#",
            ast::AttrStyle::Inner => "#!",
        });
        self.word("[");
        self.word("FIXME"); // FIXME Implement attr bodies
        self.word("]");
        self.space();
    }
}
