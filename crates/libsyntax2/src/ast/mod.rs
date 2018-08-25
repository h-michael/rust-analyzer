mod generated;

use itertools::Itertools;
use smol_str::SmolStr;

use {
    SyntaxNodeRef, SyntaxKind::*,
};
pub use self::generated::*;

pub trait AstNode<'a>: Clone + Copy + 'a {
    fn cast(syntax: SyntaxNodeRef<'a>) -> Option<Self>
        where Self: Sized;
    fn syntax(self) -> SyntaxNodeRef<'a>;
}

pub trait NameOwner<'a>: AstNode<'a> {
    fn name(self) -> Option<Name<'a>> {
        child_opt(self)
    }
}

pub trait TypeParamsOwner<'a>: AstNode<'a> {
    fn type_param_list(self) -> Option<TypeParamList<'a>> {
        child_opt(self)
    }

    fn where_clause(self) -> Option<WhereClause<'a>> {
        child_opt(self)
    }
}

pub trait AttrsOwner<'a>: AstNode<'a> {
    fn attrs(self) -> Box<Iterator<Item=Attr<'a>> + 'a> {
        Box::new(children(self))
    }
}

impl<'a> FnDef<'a> {
    pub fn has_atom_attr(&self, atom: &str) -> bool {
        self.attrs()
            .filter_map(|x| x.as_atom())
            .any(|x| x == atom)
    }
}

impl<'a> Attr<'a> {
    pub fn as_atom(&self) -> Option<SmolStr> {
        let tt = self.value()?;
        let (_bra, attr, _ket) = tt.syntax().children().collect_tuple()?;
        if attr.kind() == IDENT {
            Some(attr.leaf_text().unwrap())
        } else {
            None
        }
    }

    pub fn as_call(&self) -> Option<(SmolStr, TokenTree<'a>)> {
        let tt = self.value()?;
        let (_bra, attr, args, _ket) = tt.syntax().children().collect_tuple()?;
        let args = TokenTree::cast(args)?;
        if attr.kind() == IDENT {
            Some((attr.leaf_text().unwrap(), args))
        } else {
            None
        }
    }
}

impl<'a> Name<'a> {
    pub fn text(&self) -> SmolStr {
        let ident = self.syntax().first_child()
            .unwrap();
        ident.leaf_text().unwrap()
    }
}

impl<'a> NameRef<'a> {
    pub fn text(&self) -> SmolStr {
        let ident = self.syntax().first_child()
            .unwrap();
        ident.leaf_text().unwrap()
    }
}

impl<'a> ImplItem<'a> {
    pub fn target_type(self) -> Option<TypeRef<'a>> {
        match self.target() {
            (Some(t), None) | (_, Some(t)) => Some(t),
            _ => None,
        }
    }

    pub fn target_trait(self) -> Option<TypeRef<'a>> {
        match self.target() {
            (Some(t), Some(_)) => Some(t),
            _ => None,
        }
    }

    fn target(self) -> (Option<TypeRef<'a>>, Option<TypeRef<'a>>) {
        let mut types = children(self);
        let first = types.next();
        let second = types.next();
        (first, second)
    }
}

impl<'a> Module<'a> {
    pub fn has_semi(self) -> bool {
        match self.syntax().last_child() {
            None => false,
            Some(node) => node.kind() == SEMI,
        }
    }
}

fn child_opt<'a, P: AstNode<'a>, C: AstNode<'a>>(parent: P) -> Option<C> {
    children(parent).next()
}

fn children<'a, P: AstNode<'a>, C: AstNode<'a>>(parent: P) -> impl Iterator<Item=C> + 'a {
    parent.syntax()
        .children()
        .filter_map(C::cast)
}