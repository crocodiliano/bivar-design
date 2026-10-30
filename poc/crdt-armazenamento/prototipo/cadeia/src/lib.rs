//! The chain of one object (ADR-0001): Shape -> effects in stack order ->
//! Output, written only with the operations of ADR-0002.
//!
//! An object is a group node; its children are its stack, in order: the
//! shape first, then the effects (stroke, fill, FX...), then the output.
//! The evaluation order is the children order, so reordering the stack is
//! a single move operation and two copies reordering different items never
//! disagree about the wiring. Explicit connections are for links between
//! objects (for example one object's output as another's fill mask).

use bivar_poc_modelo::{Applied, Document, Edge, NodeId, Op, OpError, Store, Value};

pub const OBJECT: &str = "object";
pub const SHAPE: &str = "shape";
pub const OUTPUT: &str = "output";

#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub object: NodeId,
    pub shape: NodeId,
    pub stroke: NodeId,
    pub fill: NodeId,
    pub output: NodeId,
}

fn created(r: Result<Applied, OpError>) -> Result<NodeId, OpError> {
    Ok(r?.created.expect("create returns the id"))
}

/// Builds an object with Shape -> Stroke -> Fill -> Output, as one undo step.
pub fn build_chain<S: Store>(
    doc: &mut Document<S>,
    parent: Option<&NodeId>,
) -> Result<Chain, OpError> {
    let object = created(doc.apply(Op::CreateNode {
        parent: parent.cloned(),
        index: None,
        kind: OBJECT.into(),
    }))?;
    let child = |doc: &mut Document<S>, kind: &str| {
        created(doc.apply(Op::CreateNode {
            parent: Some(object.clone()),
            index: None,
            kind: kind.into(),
        }))
    };
    let shape = child(doc, SHAPE)?;
    let stroke = child(doc, "stroke")?;
    let fill = child(doc, "fill")?;
    let output = child(doc, OUTPUT)?;
    doc.apply(Op::SetParam {
        node: stroke.clone(),
        key: "width".into(),
        value: Some(Value::Num(2.0)),
    })?;
    doc.apply(Op::SetParam {
        node: fill.clone(),
        key: "color".into(),
        value: Some(Value::Str("#7c3aed".into())),
    })?;
    doc.commit();
    Ok(Chain {
        object,
        shape,
        stroke,
        fill,
        output,
    })
}

/// The order in which the object's nodes are evaluated: its stack.
pub fn evaluation_order<S: Store>(s: &S, object: &NodeId) -> Vec<(NodeId, String)> {
    s.children(Some(object))
        .into_iter()
        .map(|c| {
            let kind = s.kind(&c).unwrap_or_default();
            (c, kind)
        })
        .collect()
}

/// Whether the stack is one the layers panel can show: the shape first,
/// the output last, exactly one of each. Otherwise the layer shows the
/// "customized" indicator and the panel does not touch the graph.
pub fn is_customized<S: Store>(s: &S, object: &NodeId) -> bool {
    let order = evaluation_order(s, object);
    let count = |k: &str| order.iter().filter(|(_, kind)| kind == k).count();
    let first_is_shape = order.first().is_some_and(|(_, k)| k == SHAPE);
    let last_is_output = order.last().is_some_and(|(_, k)| k == OUTPUT);
    !(first_is_shape && last_is_output && count(SHAPE) == 1 && count(OUTPUT) == 1)
}

/// Moves an effect to `index` among the effects (between the shape and the
/// output). The panel's reorder becomes this one operation.
pub fn reorder_effect<S: Store>(
    doc: &mut Document<S>,
    object: &NodeId,
    effect: &NodeId,
    index: usize,
) -> Result<(), OpError> {
    let order = evaluation_order(&doc.store, object);
    let kind = order
        .iter()
        .find(|(n, _)| n == effect)
        .map(|(_, k)| k.as_str());
    match kind {
        None => return Err(OpError::NotFound(effect.0.clone())),
        Some(SHAPE) | Some(OUTPUT) => {
            return Err(OpError::Invalid(
                "the shape and the output do not move in the stack".into(),
            ));
        }
        Some(_) => {}
    }
    let effects = order.len().saturating_sub(3); // others, without shape, output and itself
    let index = 1 + index.min(effects);
    doc.apply(Op::MoveNode {
        node: effect.clone(),
        parent: Some(object.clone()),
        index,
    })?;
    doc.commit();
    Ok(())
}

/// Adds an effect just before the output.
pub fn add_effect<S: Store>(
    doc: &mut Document<S>,
    object: &NodeId,
    kind: &str,
) -> Result<NodeId, OpError> {
    let at = evaluation_order(&doc.store, object).len().saturating_sub(1);
    let n = created(doc.apply(Op::CreateNode {
        parent: Some(object.clone()),
        index: Some(at),
        kind: kind.into(),
    }))?;
    doc.commit();
    Ok(n)
}

/// Connects one object's output to an input port of a node elsewhere.
pub fn link<S: Store>(
    doc: &mut Document<S>,
    from: &NodeId,
    to: &NodeId,
    port: &str,
) -> Result<String, OpError> {
    let edge = Edge {
        from: from.clone(),
        from_port: "out".into(),
        to: to.clone(),
        to_port: port.into(),
    };
    let id = doc
        .apply(Op::Connect { id: None, edge })?
        .edge
        .expect("connect returns the id");
    doc.commit();
    Ok(id)
}

/// Connections with an end that is no longer visible: broken references,
/// computed from the merged state, so every copy gets the same list.
pub fn broken_links<S: Store>(s: &S) -> Vec<String> {
    s.edges()
        .into_iter()
        .filter(|(_, e)| s.parent(&e.from).is_none() || s.parent(&e.to).is_none())
        .map(|(id, _)| id)
        .collect()
}
