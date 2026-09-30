// port of: org.apache.commons.jexl3.parser.JJTParserState (jjtree-generated)
use super::jexl_node::NodeId;

#[derive(Debug, Default)]
pub struct JJTParserState {
    nodes: Vec<NodeId>,
    marks: Vec<usize>,
    sp: usize,
    mk: usize,
    node_created: bool,
}

impl JJTParserState {
    pub fn new() -> Self {
        JJTParserState::default()
    }

    // port of: JJTParserState.nodeCreated
    pub fn node_created(&self) -> bool {
        self.node_created
    }

    // port of: JJTParserState.reset
    pub fn reset(&mut self) {
        self.nodes.clear();
        self.marks.clear();
        self.sp = 0;
        self.mk = 0;
    }

    // port of: JJTParserState.pushNode
    pub fn push_node(&mut self, n: NodeId) {
        self.nodes.push(n);
        self.sp += 1;
    }

    // port of: JJTParserState.popNode
    pub fn pop_node(&mut self) -> Option<NodeId> {
        self.sp = self.sp.saturating_sub(1);
        if self.sp < self.mk {
            self.mk = self.marks.pop().unwrap_or(0);
        }
        self.nodes.pop()
    }

    // port of: JJTParserState.nodeArity
    pub fn node_arity(&self) -> i32 {
        (self.sp - self.mk) as i32
    }

    // port of: JJTParserState.clearNodeScope
    pub fn clear_node_scope(&mut self, _n: NodeId) {
        while self.sp > self.mk {
            self.pop_node();
        }
        self.mk = self.marks.pop().unwrap_or(0);
    }

    // port of: JJTParserState.openNodeScope
    pub fn open_node_scope(&mut self, _n: NodeId) {
        self.marks.push(self.mk);
        self.mk = self.sp;
    }

    /// port of: JJTParserState.closeNodeScope(Node, int) — returns the children, in order
    pub fn close_node_scope_num(&mut self, num: i32) -> Vec<NodeId> {
        self.mk = self.marks.pop().unwrap_or(0);
        let mut children = vec![0; num.max(0) as usize];
        let mut n = num;
        while n > 0 {
            n -= 1;
            children[n as usize] = self.pop_node().unwrap_or(0);
        }
        self.node_created = true;
        children
    }

    /// port of: JJTParserState.closeNodeScope(Node, boolean) — None when the node is not created
    pub fn close_node_scope_cond(&mut self, condition: bool) -> Option<Vec<NodeId>> {
        if condition {
            let a = self.node_arity();
            self.mk = self.marks.pop().unwrap_or(0);
            let mut children = vec![0; a.max(0) as usize];
            let mut n = a;
            while n > 0 {
                n -= 1;
                children[n as usize] = self.pop_node().unwrap_or(0);
            }
            self.node_created = true;
            Some(children)
        } else {
            self.mk = self.marks.pop().unwrap_or(0);
            self.node_created = false;
            None
        }
    }
}
