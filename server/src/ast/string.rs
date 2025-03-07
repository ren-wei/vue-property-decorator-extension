use swc_common::BytePos;
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::{error::Error, lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

use crate::renderer::multi_threaded_comment::MultiThreadedComments;

pub fn parse_source(
    source: &str,
    start_pos: usize,
    end_pos: usize,
) -> (Result<Module, Error>, MultiThreadedComments) {
    let input = StringInput::new(
        &source[start_pos..end_pos],
        BytePos(start_pos as u32),
        BytePos(end_pos as u32),
    );
    let syntax = Syntax::Typescript(TsSyntax {
        tsx: false,
        decorators: true,
        dts: false,
        no_early_errors: false,
        disallow_ambiguous_jsx_like: true,
    });
    let comments = MultiThreadedComments::default();
    let lexer = Lexer::new(syntax, EsVersion::EsNext, input, Some(&comments));
    let mut parser = Parser::new_from(lexer);
    let module = parser.parse_module();

    (module, comments)
}
