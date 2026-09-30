// port of: org.apache.commons.jexl3.internal.Debugger
use crate::jexl_info::Detail;
use crate::parser::jexl_node::NodeHandle;

pub struct Debugger;

impl Debugger {
    /// port of: JexlException.detailedInfo — the JexlInfo.Detail a Debugger run yields for a node.
    // ponytail: the Debugger is not ported yet, so exceptions carry no source snippet; the
    // messages that include one (runtime errors marked on a node) are still red.
    pub fn detail_of(_node: &NodeHandle) -> Option<Detail> {
        None
    }
}
