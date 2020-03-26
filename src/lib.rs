use core::ptr::NonNull;
use core::mem::MaybeUninit;
use core::marker::PhantomData;
use core::borrow::Borrow;

const MAX_N: usize = 5;

pub trait Region {
    fn collapses_with(&self, other: &Self) -> bool;
}

pub struct Tree<R, V> {
    root: Root<R, V>,
}

impl<R, V> Tree<R, V> {
    pub fn new() -> Self {
        Tree { root: Root::empty() }
    }

    pub fn len(&self) -> usize {
        self.root.size
    }

    pub fn is_empty(&self) -> bool {
        self.root.size == 0
    }

    pub fn iter(&self) -> Iter<R, V> {
        let idx = Vec::with_capacity(self.root.height);
        let cur = self.root.node
            .map(|raw| NodePtr::from_raw(raw, 0));
        Iter { idx, cur, _marker: PhantomData }
    }

    pub fn search<'a, S: ?Sized>(&'a self, region: &'a S) -> Search<'a, R, S, V>
    where 
        R: Borrow<S>, 
        S: Region 
    {
        let idx = Vec::with_capacity(self.root.height);
        let cur = self.root.node
            .map(|raw| NodePtr::from_raw(raw, 0));
        Search {
            region: region.borrow(),
            idx,
            cur,
            _marker: PhantomData
        }
    }

    pub fn insert(&mut self, region: R, value: V) -> Option<V> 
    where 
        R: Region
    {
        todo!()
    }

    pub fn remove<'a, S: ?Sized>(&'a self, region: &'a S) -> Option<V>
    where 
        R: Borrow<S>, 
        S: Region 
    {
        todo!()
    }
}

struct Root<R, V> {
    node: Option<NonNull<Node<R, V>>>,
    height: usize,
    size: usize,
    _ownership: PhantomData<Box<Node<R, V>>>
}

impl<R, V> Root<R, V> {
    fn empty() -> Self {
        Root { node: None, height: 0, size: 0, _ownership: PhantomData }
    }
}

#[repr(C)]
struct Node<R, V> {
    root: NonNull<Root<R, V>>,
    parent: Option<NonNull<InternalNode<R, V>>>,
    size: usize,
    regions: [MaybeUninit<R>; MAX_N],
    _marker: PhantomData<(R, V)>,
}

impl<R, V> Node<R, V> {
    fn root_height(&self) -> usize {
        unsafe { self.root.as_ref().height }
    }
}

struct NodePtr<R, V> {
    node: NonNull<Node<R, V>>,
    height: usize,
}

impl<R, V> Clone for NodePtr<R, V> {
    fn clone(&self) -> Self {
        NodePtr {
            node: self.node.clone(),
            height: self.height
        }
    }
}

impl<R, V> NodePtr<R, V> {
    fn from_raw(node: NonNull<Node<R, V>>, height: usize) -> Self {
        NodePtr { node, height }
    }

    fn is_internal_node(&self) -> bool {
        // note(unsafe): use Copy field of a ref from pointer
        self.height != unsafe { self.node.as_ref().root_height() }
    }

    fn is_leaf_node(&self) -> bool {
        // note(unsafe): use Copy field of a ref from pointer
        self.height == unsafe { self.node.as_ref().root_height() }
    }

    fn len(&self) -> usize {
        // note(unsafe): use Copy field of a ref from pointer
        unsafe { self.node.as_ref().size }
    }

    fn as_node(&self) -> NonNull<Node<R, V>> {
        self.node.cast::<Node<R, V>>()
    }

    // note(unsafe): only internal nodes can use this cast
    unsafe fn as_internal(&self) -> NonNull<InternalNode<R, V>> {
        self.node.cast::<InternalNode<R, V>>()
    }

    // note(unsafe): only leaf nodes can use this cast
    unsafe fn as_leaf(&self) -> NonNull<LeafNode<R, V>> {
        self.node.cast::<LeafNode<R, V>>()
    }

    // note(unsafe): only internal nodes have children
    unsafe fn get_child(&self, idx: usize) -> NodePtr<R, V> {
        debug_assert!(idx < self.len(), "index must be in bound");
        NodePtr {
            height: self.height + 1,
            node: self.as_internal().as_ref().children[idx].unwrap()
        }
    }

    // note(unsafe): only leaf nodes have values
    unsafe fn get_value_ptr(&self, idx: usize) -> *const V {
        debug_assert!(idx < self.len(), "index must be in bound");
        &self.as_leaf().as_ref().values[idx] as *const MaybeUninit<V> as *const V
    }

    fn get_region(&self, idx: usize) -> *const R {
        debug_assert!(idx < self.len(), "index must be in bound");
        unsafe { &self.as_node().as_ref().regions[idx] as *const MaybeUninit<R> as *const R }
    }

    fn get_parent(&self) -> Option<NodePtr<R, V>> {
        if self.height == 0 {
            return None;
        }
        let parent = unsafe { self.as_node().as_ref().parent };
        debug_assert!(parent.is_some(), "non-root node must have a parent");
        Some(NodePtr {
            height: self.height - 1,
            node: parent.unwrap().cast()
        })
    }
}

#[repr(C)]
#[allow(unused)] // used in Node<R, V>
struct LeafNode<R, V> {
    root: NonNull<Root<R, V>>,
    parent: Option<NonNull<InternalNode<R, V>>>,
    size: usize,
    regions: [MaybeUninit<R>; MAX_N],
    values: [MaybeUninit<V>; MAX_N],
}

#[repr(C)]
#[allow(unused)] // used in Node<R, V>
struct InternalNode<R, V> {
    root: NonNull<Root<R, V>>,
    parent: Option<NonNull<InternalNode<R, V>>>,
    size: usize,
    regions: [MaybeUninit<R>; MAX_N],
    children: [Option<NonNull<Node<R, V>>>; MAX_N],
    _marker: PhantomData<Box<Node<R, V>>>,
}

pub struct Iter<'a, R, V> {
    idx: Vec<usize>,
    cur: Option<NodePtr<R, V>>,
    _marker: PhantomData<(&'a R, &'a V)>,
}

impl<'a, R, V> Iterator for Iter<'a, R, V> {
    type Item = (&'a R, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let mut node = 
            if let Some(node) = self.cur.clone() {
                node
            } else {
                return None;
            };
        debug_assert!(node.len() > 0, "node data length must be non-zero (when root length
            is zero, `self.cur` should be `None`)");
        debug_assert!(self.idx.len() > 0, "search index array must be non-empty");
        while self.idx[self.idx.len() - 1] == node.len() - 1 {
            if let Some(parent) = node.get_parent() {
                node = parent;
                self.idx.pop();
            } else {
                // node is root node, it does not have a parent
                // search finished
                return None;
            }
        }
        while node.is_internal_node() {
            debug_assert!(self.idx.len() > 0, "index array must be non-empty");
            let next_idx = self.idx.pop().unwrap() + 1;
            // on current level
            self.idx.push(next_idx);
            node = unsafe { node.get_child(next_idx) };
            // on next level
            self.idx.push(0);
        } 
        debug_assert!(node.is_leaf_node(), "children of internal nodes must be leaf nodes");
        debug_assert!(self.idx.len() > 0, "index array must be non-empty");
        let idx = self.idx[self.idx.len() - 1];
        // note(unsafe): unbounded lifetime
        let region = unsafe { &*node.get_region(idx) };
        let value = unsafe { &*node.get_value_ptr(idx) };
        Some((region, value))
    }
}
pub struct Search<'a, R, S: ?Sized, V> {
    region: &'a S,
    idx: Vec<usize>,
    cur: Option<NodePtr<R, V>>,
    _marker: PhantomData<(&'a R, &'a V)>,
}

impl<'a, R, S: ?Sized, V> Iterator for Search<'a, R, S, V> 
where 
    R: Borrow<S>, 
    S: Region 
{
    type Item = (&'a R, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let mut node = 
            if let Some(node) = self.cur.clone() {
                node
            } else {
                return None;
            };
        debug_assert!(node.len() > 0, "node data length must be non-zero (when root length
            is zero, `self.cur` should be `None`)");
        debug_assert!(self.idx.len() > 0, "search index array must be non-empty");
        
        loop {
            while self.idx[self.idx.len() - 1] == node.len() - 1 {
                if let Some(parent) = node.get_parent() {
                    node = parent;
                    self.idx.pop();
                } else {
                    // node is root node, it does not have a parent
                    // search finished
                    return None;
                }
            }
            let mut cur_idx = self.idx[self.idx.len() - 1];
            'descend: while cur_idx < node.len() {
                let cur_region = unsafe { &*node.get_region(cur_idx) };
                if cur_region.borrow().collapses_with(self.region) {
                    if node.is_internal_node() {
                        self.idx.push(cur_idx); // cur level
                        node = unsafe { node.get_child(cur_idx) };
                        self.idx.push(0); // next level
                        break 'descend;
                    }
                    if node.is_leaf_node() {
                        let value = unsafe { &*node.get_value_ptr(cur_idx) };
                        return Some((cur_region, value));
                    }
                }
                cur_idx += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // (x_min, y_min, x_max, y_max), is_inf
    #[derive(Debug)]
    struct IntInf2DRegion(usize, usize, usize, usize, bool);

    impl Region for IntInf2DRegion {
        fn collapses_with(&self, other: &Self) -> bool {
            fn point_within(px: usize, py: usize, region: &IntInf2DRegion) -> bool {
                px >= region.0 && px <= region.2 &&
                py >= region.1 && py <= region.3
            }
            self.4 || other.4 || (
                point_within(self.0, self.1, other) &&
                point_within(self.2, self.3, other)
            )
        }
    }

    #[test]
    fn tree_iter() {
        let tree: Tree<IntInf2DRegion, usize> = Tree::new();
        println!("{:?}", tree.len());
        for region in tree.iter() {
            println!("{:?}", region)
        }
    }

    #[test]
    fn tree_search() {
        let tree: Tree<IntInf2DRegion, usize> = Tree::new();
        let region = IntInf2DRegion(1, 2, 3, 4, false);
        for region in tree.search(&region) {
            println!("{:?}", region)
        }
    }
}
