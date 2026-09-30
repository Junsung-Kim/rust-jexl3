// port of: org.apache.commons.jexl3.parser.FeatureController
// The Java class is a ScriptVisitor whose visitNode does nothing, so only the visited node itself
// is inspected; a match on the node kind is the same thing.
use crate::jexl_exception::JexlException;
use crate::jexl_features::{self, JexlFeatures};
use crate::parser::jexl_node::{Ast, NodeId};
use crate::parser::parser_tree_constants::*;

pub struct FeatureController {
    features: JexlFeatures,
}

impl FeatureController {
    pub fn new(features: JexlFeatures) -> Self {
        FeatureController { features }
    }

    pub fn set_features(&mut self, f: JexlFeatures) {
        self.features = f;
    }

    pub fn get_features(&self) -> &JexlFeatures {
        &self.features
    }

    // port of: FeatureController.controlNode
    pub fn control_node(&self, ast: &Ast, id: NodeId) -> Result<(), JexlException> {
        let node = ast.node(id);
        let f = &self.features;
        match node.kind() {
            JJTARRAYACCESS => {
                if !f.supports_array_reference_expr() {
                    for child in node.children() {
                        // isArrayReferenceLiteral: a string literal or an integer number literal
                        let ok = child.is(JJTSTRINGLITERAL)
                            || (child.is(JJTNUMBERLITERAL) && child.number().map(|n| n.is_integer()).unwrap_or(false));
                        if !ok {
                            return Err(Self::feature_exception(jexl_features::ARRAY_REF_EXPR, child.jexl_info()));
                        }
                    }
                }
            }
            JJTWHILESTATEMENT | JJTDOWHILESTATEMENT | JJTFOREACHSTATEMENT => {
                if !f.supports_loops() {
                    return Err(Self::feature_exception(jexl_features::LOOP, node.jexl_info()));
                }
            }
            JJTCONSTRUCTORNODE => {
                if !f.supports_new_instance() {
                    return Err(Self::feature_exception(jexl_features::NEW_INSTANCE, node.jexl_info()));
                }
            }
            JJTMETHODNODE => {
                if !f.supports_method_call() {
                    return Err(Self::feature_exception(jexl_features::METHOD_CALL, node.jexl_info()));
                }
            }
            JJTANNOTATION => {
                if !f.supports_annotation() {
                    return Err(Self::feature_exception(jexl_features::ANNOTATION, node.jexl_info()));
                }
            }
            JJTARRAYLITERAL | JJTMAPLITERAL | JJTSETLITERAL | JJTRANGENODE => {
                if !f.supports_structured_literal() {
                    return Err(Self::feature_exception(jexl_features::STRUCTURED_LITERAL, node.jexl_info()));
                }
            }
            // NOTE: ASTSetModNode is deliberately absent, exactly like the Java visitor:
            // `%=` is not controlled by the side-effect features in 3.2.1.
            JJTASSIGNMENT | JJTSETADDNODE | JJTSETMULTNODE | JJTSETDIVNODE | JJTSETANDNODE | JJTSETORNODE
            | JJTSETXORNODE | JJTSETSUBNODE => {
                // port of: FeatureController.controlSideEffect
                let lv = node.child(0);
                if !f.supports_side_effect_global() && lv.is_global_var() {
                    return Err(Self::feature_exception(jexl_features::SIDE_EFFECT_GLOBAL, lv.jexl_info()));
                }
                if !f.supports_side_effect() {
                    return Err(Self::feature_exception(jexl_features::SIDE_EFFECT, lv.jexl_info()));
                }
            }
            _ => {}
        }
        Ok(())
    }

    // port of: FeatureController.throwFeatureException
    fn feature_exception(feature: i32, info: Option<crate::jexl_info::JexlInfo>) -> JexlException {
        JexlException::feature(info, feature, Some(&crate::java::string::JString::empty()))
    }
}
