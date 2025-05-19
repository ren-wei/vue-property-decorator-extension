use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::mpsc::{self, Sender};
use tower_lsp::{
    lsp_types::{Diagnostic, Uri},
    Client,
};

pub struct DiagnosticsManager {
    client: Client,
    count: usize,
    diags: Arc<Mutex<HashMap<Uri, Vec<Vec<Diagnostic>>>>>,
}

impl DiagnosticsManager {
    pub fn new(client: Client) -> Self {
        DiagnosticsManager {
            client,
            count: 0,
            diags: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// register in init
    pub fn register(&mut self) -> Sender<(Uri, Option<i32>, Vec<Diagnostic>)> {
        let (tx, mut rx) = mpsc::channel(1);
        let diags = Arc::clone(&self.diags);
        let count = self.count;
        let client = self.client.clone();
        tokio::spawn(async move {
            while let Some((uri, version, msg)) = rx.recv().await {
                let all_diags = {
                    let mut diags_guard = diags.lock().unwrap();
                    DiagnosticsManager::get_all_diags(&mut diags_guard, &uri, msg, count)
                };
                client.publish_diagnostics(uri, all_diags, version).await;
            }
        });
        self.count += 1;
        return tx;
    }

    fn get_all_diags<T: Clone>(
        diags_guard: &mut HashMap<Uri, Vec<Vec<T>>>,
        uri: &Uri,
        msg: Vec<T>,
        count: usize,
    ) -> Vec<T> {
        if let Some(diags) = diags_guard.get_mut(uri) {
            if diags.len() <= count {
                for _ in diags.len()..count {
                    diags.push(vec![]);
                }
                diags.push(msg);
            } else {
                diags[count] = msg;
            }
            diags
                .iter()
                .flatten()
                .map(|v| v.clone())
                .collect::<Vec<_>>()
        } else {
            let mut diags = vec![];
            for _ in 0..count {
                diags.push(vec![]);
            }
            diags.push(msg);
            let all_diags = diags
                .iter()
                .flatten()
                .map(|v| v.clone())
                .collect::<Vec<_>>();
            diags_guard.insert(uri.clone(), diags);
            all_diags
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr};

    use tower_lsp::lsp_types::Uri;

    use super::DiagnosticsManager;

    fn assert_value(list: Vec<(usize, Vec<usize>)>, expect: Vec<usize>) {
        let mut diags = HashMap::new();
        let uri = Uri::from_str("file:///test.ts").unwrap();
        let mut result = vec![];
        for (count, msg) in list {
            result = DiagnosticsManager::get_all_diags(&mut diags, &uri, msg, count);
        }
        assert_eq!(result, expect);
    }

    #[test]
    fn diags() {
        assert_value(
            vec![(2, vec![0, 1, 2]), (1, vec![3, 4, 5])],
            vec![3, 4, 5, 0, 1, 2],
        );
        assert_value(vec![(2, vec![0, 1, 2]), (2, vec![3, 4, 5])], vec![3, 4, 5]);
    }
}
