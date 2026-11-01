use crate::backend::{document::DocumentState, prelude::*};
use pdfoxium_core::{
    fpdfapi::parser::cpdf_object::ObjectRef,
    fpdfdoc::{cpdf_bookmark::CpdfBookmark, cpdf_bookmarktree::CpdfBookmarkTree},
};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
    rc::Rc,
};
struct Node {
    dict: ObjectRef,
    title: String,
    parent: Option<usize>,
    children: Vec<usize>,
}
struct Tree {
    document: Rc<DocumentState>,
    nodes: Vec<Node>,
    roots: Vec<usize>,
}
pub struct PdfBookmarks {
    tree: Rc<Tree>,
}
#[derive(Clone)]
pub struct PdfBookmark {
    tree: Rc<Tree>,
    index: usize,
    parent_known: bool,
}
impl PartialEq for PdfBookmark {
    fn eq(&self, other: &Self) -> bool {
        self.tree.document.identity == other.tree.document.identity
            && Rc::ptr_eq(
                &self.tree.nodes[self.index].dict,
                &other.tree.nodes[other.index].dict,
            )
    }
}
impl Eq for PdfBookmark {}
impl Hash for PdfBookmark {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tree.document.identity.hash(state);
        Rc::as_ptr(&self.tree.nodes[self.index].dict).hash(state);
    }
}
impl PdfDocument {
    pub fn bookmarks(&self) -> PdfBookmarks {
        let document = self.inner.core.borrow();
        let source = CpdfBookmarkTree::new(&document);
        let mut tree = Tree {
            document: self.inner.clone(),
            nodes: Vec::new(),
            roots: Vec::new(),
        };
        let mut pending = vec![(source.first_child(&CpdfBookmark::default()), None)];
        let mut seen = HashSet::new();
        while let Some((bookmark, parent)) = pending.pop() {
            let Some(dict) = bookmark.dict() else {
                continue;
            };
            if !seen.insert(Rc::as_ptr(&dict)) {
                continue;
            }
            let index = tree.nodes.len();
            tree.nodes.push(Node {
                dict,
                title: crate::backend::text::decode(&bookmark.title()),
                parent,
                children: Vec::new(),
            });
            if let Some(parent) = parent {
                tree.nodes[parent].children.push(index);
            } else {
                tree.roots.push(index);
            }
            pending.push((source.next_sibling(&bookmark), parent));
            pending.push((source.first_child(&bookmark), Some(index)));
        }
        PdfBookmarks {
            tree: Rc::new(tree),
        }
    }
}
impl PdfBookmarks {
    pub fn root(&self) -> Option<PdfBookmark> {
        self.tree.roots.first().map(|&index| PdfBookmark {
            tree: self.tree.clone(),
            index,
            parent_known: false,
        })
    }
    pub fn iter(&self) -> impl Iterator<Item = PdfBookmark> + '_ {
        (0..self.tree.nodes.len()).map(|index| PdfBookmark {
            tree: self.tree.clone(),
            index,
            parent_known: true,
        })
    }
    pub fn find_first_by_title(&self, title: &str) -> Result<PdfBookmark, PdfiumError> {
        self.tree
            .nodes
            .iter()
            .position(|n| n.title == title)
            .map(|index| PdfBookmark {
                tree: self.tree.clone(),
                index,
                parent_known: false,
            })
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
    }
    pub fn find_all_by_title(&self, title: &str) -> Vec<PdfBookmark> {
        self.iter()
            .filter(|b| b.title().as_deref() == Some(title))
            .collect()
    }
}
impl PdfBookmark {
    pub fn title(&self) -> Option<String> {
        let s = &self.tree.nodes[self.index].title;
        (!s.is_empty()).then(|| s.clone())
    }
    pub fn parent(&self) -> Option<PdfBookmark> {
        self.parent_known
            .then(|| self.tree.nodes[self.index].parent)
            .flatten()
            .map(|index| Self {
                tree: self.tree.clone(),
                index,
                parent_known: true,
            })
    }
    pub fn first_child(&self) -> Option<PdfBookmark> {
        self.tree.nodes[self.index]
            .children
            .first()
            .map(|&index| Self {
                tree: self.tree.clone(),
                index,
                parent_known: true,
            })
    }
    pub fn next_sibling(&self) -> Option<PdfBookmark> {
        let n = &self.tree.nodes[self.index];
        let siblings = n
            .parent
            .map_or(&self.tree.roots, |p| &self.tree.nodes[p].children);
        let pos = siblings.iter().position(|&i| i == self.index)?;
        siblings.get(pos + 1).map(|&index| Self {
            tree: self.tree.clone(),
            index,
            parent_known: self.parent_known,
        })
    }
    pub fn children_len(&self) -> usize {
        self.tree.nodes[self.index].children.len()
    }
    pub fn iter_direct_children(&self) -> impl Iterator<Item = PdfBookmark> + '_ {
        self.tree.nodes[self.index]
            .children
            .iter()
            .map(|&index| Self {
                tree: self.tree.clone(),
                index,
                parent_known: true,
            })
    }
    pub fn iter_all_descendants(&self) -> impl Iterator<Item = PdfBookmark> {
        let mut indices = Vec::new();
        let mut stack = self.tree.nodes[self.index]
            .children
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>();
        while let Some(index) = stack.pop() {
            indices.push(index);
            stack.extend(self.tree.nodes[index].children.iter().rev().copied());
        }
        let tree = self.tree.clone();
        indices.into_iter().map(move |index| Self {
            tree: tree.clone(),
            index,
            parent_known: true,
        })
    }
    pub fn iter_siblings(&self) -> impl Iterator<Item = PdfBookmark> {
        let mut indices = Vec::new();
        if self.parent_known {
            let node = &self.tree.nodes[self.index];
            let siblings = node
                .parent
                .map_or(&self.tree.roots, |p| &self.tree.nodes[p].children);
            indices.extend(siblings.iter().copied().filter(|&i| i != self.index));
        } else {
            let mut next = self.next_sibling();
            while let Some(b) = next {
                indices.push(b.index);
                next = b.next_sibling();
            }
        }
        let tree = self.tree.clone();
        let known = self.parent_known;
        indices.into_iter().map(move |index| Self {
            tree: tree.clone(),
            index,
            parent_known: known,
        })
    }
}

impl PdfBookmark {
    pub(crate) fn core_context(&self) -> (Rc<DocumentState>, ObjectRef) {
        (
            self.tree.document.clone(),
            self.tree.nodes[self.index].dict.clone(),
        )
    }
}
