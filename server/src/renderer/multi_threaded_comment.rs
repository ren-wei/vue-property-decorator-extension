use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use swc_atoms::atom;
use swc_common::{
    comments::{Comment, CommentKind, Comments},
    BytePos, DUMMY_SP,
};

pub type MultiThreadedCommentsMapInner = HashMap<BytePos, Vec<Comment>>;
pub type MultiThreadedCommentsMap = Arc<RwLock<MultiThreadedCommentsMapInner>>;

#[derive(Debug, Clone, Default)]
pub struct MultiThreadedComments {
    leading: MultiThreadedCommentsMap,
    trailing: MultiThreadedCommentsMap,
}

impl Comments for MultiThreadedComments {
    fn add_leading(&self, pos: BytePos, cmt: Comment) {
        self.leading
            .write()
            .unwrap()
            .entry(pos)
            .or_default()
            .push(cmt);
    }

    fn add_leading_comments(&self, pos: BytePos, comments: Vec<Comment>) {
        self.leading
            .write()
            .unwrap()
            .entry(pos)
            .or_default()
            .extend(comments);
    }

    fn has_leading(&self, pos: BytePos) -> bool {
        if let Some(v) = self.leading.read().unwrap().get(&pos) {
            !v.is_empty()
        } else {
            false
        }
    }

    fn move_leading(&self, from: BytePos, to: BytePos) {
        let cmt = self.take_leading(from);

        if let Some(mut cmt) = cmt {
            if from < to && self.has_leading(to) {
                cmt.extend(self.take_leading(to).unwrap());
            }

            self.add_leading_comments(to, cmt);
        }
    }

    fn take_leading(&self, pos: BytePos) -> Option<Vec<Comment>> {
        self.leading.write().unwrap().remove(&pos)
    }

    fn get_leading(&self, pos: BytePos) -> Option<Vec<Comment>> {
        self.leading.read().unwrap().get(&pos).map(|c| c.to_owned())
    }

    fn add_trailing(&self, pos: BytePos, cmt: Comment) {
        self.trailing
            .write()
            .unwrap()
            .entry(pos)
            .or_default()
            .push(cmt);
    }

    fn add_trailing_comments(&self, pos: BytePos, comments: Vec<Comment>) {
        self.trailing
            .write()
            .unwrap()
            .entry(pos)
            .or_default()
            .extend(comments);
    }

    fn has_trailing(&self, pos: BytePos) -> bool {
        if let Some(v) = self.trailing.read().unwrap().get(&pos) {
            !v.is_empty()
        } else {
            false
        }
    }

    fn move_trailing(&self, from: BytePos, to: BytePos) {
        let cmt = self.take_trailing(from);

        if let Some(mut cmt) = cmt {
            if from < to && self.has_trailing(to) {
                cmt.extend(self.take_trailing(to).unwrap());
            }

            self.add_trailing_comments(to, cmt);
        }
    }

    fn take_trailing(&self, pos: BytePos) -> Option<Vec<Comment>> {
        self.trailing.write().unwrap().remove(&pos)
    }

    fn get_trailing(&self, pos: BytePos) -> Option<Vec<Comment>> {
        self.trailing
            .read()
            .unwrap()
            .get(&pos)
            .map(|c| c.to_owned())
    }

    fn add_pure_comment(&self, pos: BytePos) {
        assert_ne!(pos, BytePos(0), "cannot add pure comment to zero position");

        let mut leading_map = self.leading.write().unwrap();
        let leading = leading_map.entry(pos).or_default();
        let pure_comment = Comment {
            kind: CommentKind::Block,
            span: DUMMY_SP,
            text: atom!("#__PURE__"),
        };

        if !leading.iter().any(|c| c.text == pure_comment.text) {
            leading.push(pure_comment);
        }
    }

    fn with_leading<F, Ret>(&self, pos: BytePos, f: F) -> Ret
    where
        Self: Sized,
        F: FnOnce(&[Comment]) -> Ret,
    {
        let b = self.leading.read().unwrap();
        let cmts = b.get(&pos);

        if let Some(cmts) = &cmts {
            f(cmts)
        } else {
            f(&[])
        }
    }

    fn with_trailing<F, Ret>(&self, pos: BytePos, f: F) -> Ret
    where
        Self: Sized,
        F: FnOnce(&[Comment]) -> Ret,
    {
        let b = self.trailing.read().unwrap();
        let cmts = b.get(&pos);

        if let Some(cmts) = &cmts {
            f(cmts)
        } else {
            f(&[])
        }
    }

    fn has_flag(&self, lo: BytePos, flag: &str) -> bool {
        self.with_leading(lo, |comments| {
            for c in comments {
                if c.kind == CommentKind::Block {
                    for line in c.text.lines() {
                        // jsdoc
                        let line = line.trim_start_matches(['*', ' ']);
                        let line = line.trim();

                        //
                        if line.len() == (flag.len() + 5)
                            && (line.starts_with("#__") || line.starts_with("@__"))
                            && line.ends_with("__")
                            && flag == &line[3..line.len() - 2]
                        {
                            return true;
                        }
                    }
                }
            }

            false
        })
    }
}
