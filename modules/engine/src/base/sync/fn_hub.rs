use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;

type FnBox<T, Args> = Box<dyn Fn(&Args) -> T + Send + Sync>;

#[allow(unused)]
pub struct FnHub<T, Args> {
    tasks: Arc<Mutex<HashMap<u32, FnBox<T, Args>>>>,
    next_id: Arc<Mutex<u32>>,
}

#[allow(unused)]
impl<T, Args> FnHub<T, Args> {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    pub fn caller(&self) -> FnCaller<T, Args> {
        FnCaller { tasks: Arc::clone(&self.tasks) }
    }

    pub fn listener(&self) -> FnListener<T, Args> {
        FnListener {
            tasks: Arc::clone(&self.tasks),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

impl<T, Args> Default for FnHub<T, Args> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct FnCaller<T, Args> {
    tasks: Arc<Mutex<HashMap<u32, FnBox<T, Args>>>>,
}

impl<T, Args> FnCaller<T, Args> {
    pub fn call(&self, args: &Args) -> Vec<T> {
        let tasks = self.tasks.lock();
        tasks.values().map(|f| f(args)).collect()
    }
}

#[allow(unused)]
#[derive(Clone)]
pub struct FnListener<T, Args> {
    tasks: Arc<Mutex<HashMap<u32, FnBox<T, Args>>>>,
    next_id: Arc<Mutex<u32>>,
}

#[allow(unused)]
impl<T, Args> FnListener<T, Args> {
    pub fn listen<F>(&self, f: F) -> FnHandle<T, Args>
    where
        F: Fn(&Args) -> T + Send + Sync + 'static,
    {
        let mut tasks = self.tasks.lock();
        let mut id = self.next_id.lock();
        let current_id = *id;
        tasks.insert(current_id, Box::new(f));
        *id += 1;
        FnHandle {
            tasks: Arc::clone(&self.tasks),
            id: current_id,
        }
    }
}

pub struct FnHandle<T, Args> {
    tasks: Arc<Mutex<HashMap<u32, FnBox<T, Args>>>>,
    id: u32,
}

impl<T, Args> Drop for FnHandle<T, Args> {
    fn drop(&mut self) {
        let mut tasks = self.tasks.lock();
        tasks.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_test() {
        let hub = FnHub::<i32, ()>::new();
        let registrar = hub.listener();
        let _cookie = registrar.listen(|_| 42);
        assert_eq!(registrar.tasks.lock().len(), 1);
        let executor = hub.caller();
        assert_eq!(executor.call(&()), vec![42]);
    }

    #[test]
    fn cookie_drop_test() {
        let hub = FnHub::<i32, ()>::new();
        let registrar = hub.listener();
        {
            let _cookie = registrar.listen(|_| 42);
        } // _cookie is dropped here
        assert!(registrar.tasks.lock().is_empty());
    }
}
