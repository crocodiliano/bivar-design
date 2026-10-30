//! Operations (ADR-0002) and their serialized form (ADR-0021).
//!
//! Every change to the document is one of these, whether it comes from the
//! interface, a script, a colleague or undo. They name nodes by `NodeId` and
//! carry plain values: no type from the CRDT library appears here.

use std::fmt;

/// Version of the serialized operation format. A reader refuses any other
/// version with a clear error, never guesses.
pub const OP_FORMAT_VERSION: u32 = 1;

/// A node identity, unique across machines. Issued by the store when the
/// node is created; opaque to everything else.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub String);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        NodeId(s.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
}

/// A connection from an output port to an input port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: NodeId,
    pub from_port: String,
    pub to: NodeId,
    pub to_port: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// `index: None` appends after the last sibling.
    CreateNode {
        parent: Option<NodeId>,
        index: Option<usize>,
        kind: String,
    },
    /// Marks the node removed. It stays in the document (with its data and
    /// identity) so a removal can be undone and a conflict can restore it.
    RemoveNode {
        node: NodeId,
    },
    RestoreNode {
        node: NodeId,
    },
    /// Moves under `parent` at `index` among its other children. Reordering
    /// a stack is a move to the same parent.
    MoveNode {
        node: NodeId,
        parent: Option<NodeId>,
        index: usize,
    },
    /// `value: None` clears the parameter.
    SetParam {
        node: NodeId,
        key: String,
        value: Option<Value>,
    },
    /// `id: None` lets the store issue a new edge id.
    Connect {
        id: Option<String>,
        edge: Edge,
    },
    Disconnect {
        id: String,
    },
    InsertText {
        node: NodeId,
        pos: usize,
        text: String,
    },
    DeleteText {
        node: NodeId,
        pos: usize,
        len: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodeError {
    WrongVersion(u32),
    Malformed(&'static str),
}

/// Length-prefixed fields, so any text (separators included) round-trips.
struct Writer(String);

impl Writer {
    fn field(&mut self, s: &str) {
        self.0.push_str(&s.len().to_string());
        self.0.push(':');
        self.0.push_str(s);
    }

    fn opt_node(&mut self, n: &Option<NodeId>) {
        match n {
            Some(n) => self.field(&format!("+{}", n.0)),
            None => self.field("-"),
        }
    }

    fn value(&mut self, v: &Option<Value>) {
        match v {
            None => self.field("-"),
            Some(Value::Num(n)) => self.field(&format!("n{}", n.to_bits())),
            Some(Value::Str(s)) => self.field(&format!("s{s}")),
        }
    }
}

struct Reader<'a>(&'a str);

impl<'a> Reader<'a> {
    fn field(&mut self) -> Result<&'a str, DecodeError> {
        let colon = self
            .0
            .find(':')
            .ok_or(DecodeError::Malformed("missing length"))?;
        let len: usize = self.0[..colon]
            .parse()
            .map_err(|_| DecodeError::Malformed("bad length"))?;
        let rest = &self.0[colon + 1..];
        let s = rest
            .get(..len)
            .ok_or(DecodeError::Malformed("short field"))?;
        self.0 = &rest[len..];
        Ok(s)
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        Ok(self.field()?.to_owned())
    }

    fn node(&mut self) -> Result<NodeId, DecodeError> {
        Ok(NodeId(self.string()?))
    }

    fn usize(&mut self) -> Result<usize, DecodeError> {
        self.field()?
            .parse()
            .map_err(|_| DecodeError::Malformed("bad number"))
    }

    fn opt_node(&mut self) -> Result<Option<NodeId>, DecodeError> {
        let f = self.field()?;
        match f.strip_prefix('+') {
            Some(n) => Ok(Some(NodeId(n.to_owned()))),
            None if f == "-" => Ok(None),
            None => Err(DecodeError::Malformed("bad optional node")),
        }
    }

    fn value(&mut self) -> Result<Option<Value>, DecodeError> {
        let f = self.field()?;
        if f == "-" {
            return Ok(None);
        }
        if let Some(bits) = f.strip_prefix('n') {
            let bits: u64 = bits
                .parse()
                .map_err(|_| DecodeError::Malformed("bad number"))?;
            return Ok(Some(Value::Num(f64::from_bits(bits))));
        }
        match f.strip_prefix('s') {
            Some(s) => Ok(Some(Value::Str(s.to_owned()))),
            None => Err(DecodeError::Malformed("bad value")),
        }
    }
}

impl Op {
    /// Serialized form: `bivar-op/<version>/` followed by the fields.
    pub fn encode(&self) -> String {
        let mut w = Writer(format!("bivar-op/{OP_FORMAT_VERSION}/"));
        match self {
            Op::CreateNode {
                parent,
                index,
                kind,
            } => {
                w.field("create");
                w.opt_node(parent);
                w.field(&index.map_or("-".to_owned(), |i| i.to_string()));
                w.field(kind);
            }
            Op::RemoveNode { node } => {
                w.field("remove");
                w.field(&node.0);
            }
            Op::RestoreNode { node } => {
                w.field("restore");
                w.field(&node.0);
            }
            Op::MoveNode {
                node,
                parent,
                index,
            } => {
                w.field("move");
                w.field(&node.0);
                w.opt_node(parent);
                w.field(&index.to_string());
            }
            Op::SetParam { node, key, value } => {
                w.field("set");
                w.field(&node.0);
                w.field(key);
                w.value(value);
            }
            Op::Connect { id, edge } => {
                w.field("connect");
                w.field(
                    id.as_deref()
                        .map_or("-".to_owned(), |i| format!("+{i}"))
                        .as_str(),
                );
                for f in [&edge.from.0, &edge.from_port, &edge.to.0, &edge.to_port] {
                    w.field(f);
                }
            }
            Op::Disconnect { id } => {
                w.field("disconnect");
                w.field(id);
            }
            Op::InsertText { node, pos, text } => {
                w.field("insert-text");
                w.field(&node.0);
                w.field(&pos.to_string());
                w.field(text);
            }
            Op::DeleteText { node, pos, len } => {
                w.field("delete-text");
                w.field(&node.0);
                w.field(&pos.to_string());
                w.field(&len.to_string());
            }
        }
        w.0
    }

    pub fn decode(s: &str) -> Result<Op, DecodeError> {
        let rest = s
            .strip_prefix("bivar-op/")
            .ok_or(DecodeError::Malformed("not an operation"))?;
        let slash = rest.find('/').ok_or(DecodeError::Malformed("no version"))?;
        let version: u32 = rest[..slash]
            .parse()
            .map_err(|_| DecodeError::Malformed("bad version"))?;
        if version != OP_FORMAT_VERSION {
            return Err(DecodeError::WrongVersion(version));
        }
        let mut r = Reader(&rest[slash + 1..]);
        let op = match r.field()? {
            "create" => Op::CreateNode {
                parent: r.opt_node()?,
                index: match r.field()? {
                    "-" => None,
                    i => Some(i.parse().map_err(|_| DecodeError::Malformed("bad index"))?),
                },
                kind: r.string()?,
            },
            "remove" => Op::RemoveNode { node: r.node()? },
            "restore" => Op::RestoreNode { node: r.node()? },
            "move" => Op::MoveNode {
                node: r.node()?,
                parent: r.opt_node()?,
                index: r.usize()?,
            },
            "set" => Op::SetParam {
                node: r.node()?,
                key: r.string()?,
                value: r.value()?,
            },
            "connect" => {
                let id = match r.field()? {
                    "-" => None,
                    i => Some(
                        i.strip_prefix('+')
                            .ok_or(DecodeError::Malformed("bad edge id"))?
                            .to_owned(),
                    ),
                };
                Op::Connect {
                    id,
                    edge: Edge {
                        from: r.node()?,
                        from_port: r.string()?,
                        to: r.node()?,
                        to_port: r.string()?,
                    },
                }
            }
            "disconnect" => Op::Disconnect { id: r.string()? },
            "insert-text" => Op::InsertText {
                node: r.node()?,
                pos: r.usize()?,
                text: r.string()?,
            },
            "delete-text" => Op::DeleteText {
                node: r.node()?,
                pos: r.usize()?,
                len: r.usize()?,
            },
            _ => return Err(DecodeError::Malformed("unknown operation")),
        };
        if !r.0.is_empty() {
            return Err(DecodeError::Malformed("trailing data"));
        }
        Ok(op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> NodeId {
        NodeId::from(s)
    }

    #[test]
    fn every_operation_round_trips() {
        let ops = [
            Op::CreateNode {
                parent: None,
                index: None,
                kind: "shape".into(),
            },
            Op::CreateNode {
                parent: Some(n("3@7")),
                index: Some(2),
                kind: "stroke".into(),
            },
            Op::RemoveNode { node: n("1@2") },
            Op::RestoreNode { node: n("1@2") },
            Op::MoveNode {
                node: n("1@2"),
                parent: None,
                index: 0,
            },
            Op::SetParam {
                node: n("1@2"),
                key: "color".into(),
                value: Some(Value::Str("a:b\u{1f}c".into())),
            },
            Op::SetParam {
                node: n("1@2"),
                key: "x".into(),
                value: Some(Value::Num(-0.1)),
            },
            Op::SetParam {
                node: n("1@2"),
                key: "x".into(),
                value: None,
            },
            Op::Connect {
                id: Some("e1".into()),
                edge: Edge {
                    from: n("1@2"),
                    from_port: "out".into(),
                    to: n("2@2"),
                    to_port: "in".into(),
                },
            },
            Op::Disconnect { id: "e1".into() },
            Op::InsertText {
                node: n("4@1"),
                pos: 3,
                text: "Açaí 12:34".into(),
            },
            Op::DeleteText {
                node: n("4@1"),
                pos: 0,
                len: 2,
            },
        ];
        for op in ops {
            assert_eq!(Op::decode(&op.encode()), Ok(op.clone()), "{}", op.encode());
        }
    }

    #[test]
    fn another_format_version_is_refused() {
        let s = Op::RemoveNode { node: n("1@2") }
            .encode()
            .replace("bivar-op/1/", "bivar-op/2/");
        assert_eq!(Op::decode(&s), Err(DecodeError::WrongVersion(2)));
    }

    #[test]
    fn malformed_input_is_refused_not_panicking() {
        for s in [
            "",
            "bivar-op/1/",
            "bivar-op/1/6:remove",
            "bivar-op/1/99:remove",
            "bivar-op/x/",
            "bivar-op/1/6:remove3:1@2xx",
        ] {
            assert!(Op::decode(s).is_err(), "{s}");
        }
    }
}
