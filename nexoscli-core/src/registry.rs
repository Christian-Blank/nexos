use std::collections::HashMap;

use clap::Command;

use crate::plugin::Plugin;

pub struct PluginRegistry {
    plugins: HashMap<&'static str, Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let name = plugin.name();
        self.plugins.insert(name, plugin);
    }

    pub fn iter(&self) -> impl Iterator<Item=&dyn Plugin> {
        self.plugins.values().map(|b| b.as_ref())
    }

    pub fn get(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins.get(name).map(|b| b.as_ref())
    }

    pub fn augment_all(&self, mut app: Command) -> Command {
        for p in self.iter() {
            app = p.augment_commands(app);
        }
        app
    }
}
