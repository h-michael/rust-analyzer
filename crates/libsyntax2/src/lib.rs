//! An experimental implementation of [Rust RFC#2256 libsyntax2.0][rfc#2256].
//!
//! The intent is to be an IDE-ready parser, i.e. one that offers
//!
//! - easy and fast incremental re-parsing,
//! - graceful handling of errors, and
//! - maintains all information in the source file.
//!
//! For more information, see [the RFC][rfc#2265], or [the working draft][RFC.md].
//!
//!   [rfc#2256]: <https://github.com/rust-lang/rfcs/pull/2256>
//!   [RFC.md]: <https://github.com/matklad/libsyntax2/blob/master/docs/RFC.md>

#![forbid(
    missing_debug_implementations,
    unconditional_recursion,
    future_incompatible
)]
#![deny(bad_style, missing_docs)]
#![allow(missing_docs)]
//#![warn(unreachable_pub)] // rust-lang/rust#47816

extern crate itertools;
extern crate unicode_xid;
extern crate drop_bomb;
extern crate parking_lot;
extern crate smol_str;
extern crate text_unit;

pub mod algo;
pub mod ast;
mod lexer;
#[macro_use]
mod parser_api;
mod grammar;
mod parser_impl;

mod syntax_kinds;
mod yellow;
/// Utilities for simple uses of the parser.
pub mod utils;
pub mod text_utils;

pub use {
    text_unit::{TextRange, TextUnit},
    smol_str::SmolStr,
    ast::AstNode,
    lexer::{tokenize, Token},
    syntax_kinds::SyntaxKind,
    yellow::{SyntaxNode, SyntaxNodeRef, OwnedRoot, RefRoot, TreeRoot, SyntaxError},
};

use {
    SyntaxKind::*,
    yellow::{GreenNode, SyntaxRoot},
    parser_api::Parser,
};

#[derive(Clone, Debug)]
pub struct File {
    root: SyntaxNode
}

impl File {
    fn new(green: GreenNode, errors: Vec<SyntaxError>) -> File {
        let root = SyntaxRoot::new(green, errors);
        let root = SyntaxNode::new_owned(root);
        validate_block_structure(root.borrowed());
        File { root }
    }
    pub fn parse(text: &str) -> File {
        let tokens = tokenize(&text);
        let (green, errors) = parser_impl::parse::<yellow::GreenBuilder>(text, &tokens);
        File::new(green, errors)
    }
    pub fn reparse(&self, edit: &AtomEdit) -> File {
        self.incremental_reparse(edit).unwrap_or_else(|| self.full_reparse(edit))
    }
    fn incremental_reparse(&self, edit: &AtomEdit) -> Option<File> {
        let (node, reparser) = find_reparsable_node(self.syntax(), edit.delete)?;
        let text = replace_range(
            node.text(),
            edit.delete - node.range().start(),
            &edit.insert,
        );
        let tokens = tokenize(&text);
        if !is_balanced(&tokens) {
            return None;
        }
        None
    }
    fn full_reparse(&self, edit: &AtomEdit) -> File {
        let text = replace_range(self.syntax().text(), edit.delete, &edit.insert);
        File::parse(&text)
    }
    pub fn ast(&self) -> ast::Root {
        ast::Root::cast(self.syntax()).unwrap()
    }
    pub fn syntax(&self) -> SyntaxNodeRef {
        self.root.borrowed()
    }
    pub fn errors(&self) -> Vec<SyntaxError> {
        self.syntax().root.syntax_root().errors.clone()
    }
}

#[cfg(not(debug_assertions))]
fn validate_block_structure(_: SyntaxNodeRef) {}

#[cfg(debug_assertions)]
fn validate_block_structure(root: SyntaxNodeRef) {
    let mut stack = Vec::new();
    for node in algo::walk::preorder(root) {
        match node.kind() {
            SyntaxKind::L_CURLY => {
                stack.push(node)
            }
            SyntaxKind::R_CURLY => {
                if let Some(pair) = stack.pop() {
                    assert_eq!(
                        node.parent(),
                        pair.parent(),
                        "unpaired curleys:\n{}",
                        utils::dump_tree(root),
                    );
                    assert!(
                        node.next_sibling().is_none() && pair.prev_sibling().is_none(),
                        "floating curlys at {:?}\nfile:\n{}\nerror:\n{}\n",
                        node,
                        root.text(),
                        node.text(),
                    );
                }
            }
            _ => (),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AtomEdit {
    pub delete: TextRange,
    pub insert: String,
}

impl AtomEdit {
    pub fn replace(range: TextRange, replace_with: String) -> AtomEdit {
        AtomEdit { delete: range, insert: replace_with }
    }

    pub fn delete(range: TextRange) -> AtomEdit {
        AtomEdit::replace(range, String::new())
    }

    pub fn insert(offset: TextUnit, text: String) -> AtomEdit {
        AtomEdit::replace(TextRange::offset_len(offset, 0.into()), text)
    }
}

fn find_reparsable_node(node: SyntaxNodeRef, range: TextRange) -> Option<(SyntaxNodeRef, fn(&mut Parser))> {
    let node = algo::find_covering_node(node, range);
    return algo::ancestors(node)
        .filter_map(|node| reparser(node).map(|r| (node, r)))
        .next();

    fn reparser(node: SyntaxNodeRef) -> Option<fn(&mut Parser)> {
        let res = match node.kind() {
            BLOCK => grammar::block,
            NAMED_FIELD_DEF_LIST => grammar::named_field_def_list,
            _ => return None,
        };
        Some(res)
    }
}

fn replace_range(mut text: String, range: TextRange, replace_with: &str) -> String {
    let start = u32::from(range.start()) as usize;
    let end = u32::from(range.end()) as usize;
    text.replace_range(start..end, replace_with);
    text
}

fn is_balanced(tokens: &[Token]) -> bool {
    if tokens.len() == 0
       || tokens.first().unwrap().kind != L_CURLY
       || tokens.last().unwrap().kind != R_CURLY {
        return false
    }
    let mut balance = 0usize;
    for t in tokens.iter() {
        match t.kind {
            L_CURLY => balance += 1,
            R_CURLY => balance = match balance.checked_sub(1) {
                Some(b) => b,
                None => return false,
            },
            _ => (),
        }
    }
    balance == 0
}