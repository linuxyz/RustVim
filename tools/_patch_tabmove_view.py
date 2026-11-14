import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("""    pub tabs: crate::arena_map::ArenaMap<TabId, TabPage>,
    pub current_tab: TabId,""", """    pub tabs: crate::arena_map::ArenaMap<TabId, TabPage>,
    pub tab_order: Vec<TabId>,
    pub current_tab: TabId,""", 1)
s = s.replace("""        tabs.insert(
            tab_id,
            TabPage {
                id: tab_id,
                root: LayoutNode::leaf(window_id),
                current: window_id,
                cwd: None,
            },
        );""", """        tabs.insert(
            tab_id,
            TabPage {
                id: tab_id,
                root: LayoutNode::leaf(window_id),
                current: window_id,
                cwd: None,
            },
        );""", 1)
# init tab_order after tabs insert? Need locate struct init end. Add after tabs.insert block in new()
s = s.replace("""        ViewState {
            windows,
            tabs,
            current_tab: tab_id,""", """        ViewState {
            windows,
            tabs,
            tab_order: vec![tab_id],
            current_tab: tab_id,""", 1)
s = s.replace("""        self.tabs.insert(
            id,
            TabPage {
                id,
                root: LayoutNode::leaf(window_id),
                current: window_id,
                cwd: None,
            },
        );
        self.current_tab = id;""", """        self.tabs.insert(
            id,
            TabPage {
                id,
                root: LayoutNode::leaf(window_id),
                current: window_id,
                cwd: None,
            },
        );
        self.tab_order.push(id);
        self.current_tab = id;""", 1)
s = s.replace("""    pub fn tab_ids(&self) -> Vec<TabId> {
        self.tabs.keys().collect()
    }""", """    pub fn tab_ids(&self) -> Vec<TabId> {
        let mut order: Vec<TabId> = self
            .tab_order
            .iter()
            .copied()
            .filter(|id| self.tabs.contains_key(id))
            .collect();
        for id in self.tabs.keys() {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        order
    }

    pub fn move_current_tab(&mut self, target: usize) {
        let ids = self.tab_ids();
        let Some(index) = ids.iter().position(|id| *id == self.current_tab) else {
            return;
        };
        let target = target.min(ids.len().saturating_sub(1));
        if index == target {
            return;
        }
        let current = self.current_tab;
        self.tab_order = ids;
        self.tab_order.remove(index);
        self.tab_order.insert(target, current);
    }""", 1)
s = s.replace("""    pub fn next_tab(&mut self) -> TabId {
        let ids: Vec<TabId> = self.tabs.keys().collect();""", """    pub fn next_tab(&mut self) -> TabId {
        let ids: Vec<TabId> = self.tab_ids();""", 1)
s = s.replace("""    pub fn previous_tab(&mut self) -> TabId {
        let ids: Vec<TabId> = self.tabs.keys().collect();""", """    pub fn previous_tab(&mut self) -> TabId {
        let ids: Vec<TabId> = self.tab_ids();""", 1)
s = s.replace("""        self.current_tab = self.tabs.keys().next().unwrap_or(self.current_tab);
        true""", """        self.tab_order.retain(|item| *item != tab.id);
        self.current_tab = self.tab_ids().first().copied().unwrap_or(self.current_tab);
        true""", 1)
s = s.replace("""        let remove: Vec<TabId> = self.tabs.keys().filter(|id| *id != current).collect();""", """        let remove: Vec<TabId> = self.tab_ids().into_iter().filter(|id| *id != current).collect();""", 1)
s = s.replace("""            if let Some(page) = self.tabs.remove(&tab) {
                for window in crate::layout::all_windows(&page.root) {""", """            if let Some(page) = self.tabs.remove(&tab) {
                self.tab_order.retain(|item| *item != tab);
                for window in crate::layout::all_windows(&page.root) {""", 1)
p.write_text(s)
print("view patched")
