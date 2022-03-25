use std::thread;
use std::sync::Mutex;
use std::collections::HashMap;

use static_arc::StaticArc;

struct SharedData {
    data: Mutex<HashMap<String, i32>>,
}

fn main() {
    let [thread_ptr, main_ptr] = StaticArc::new(SharedData {
        data: Mutex::new(HashMap::new()),
    }).unwrap();

    thread::spawn(move || {
        let mut data = thread_ptr.data.lock().unwrap();
        data.insert("x".into(), 1);
        data.insert("y".into(), 2);
        data.insert("z".into(), 3);
    });

    let data = recover(main_ptr);

    for (k, v) in data.iter() {
        println!("{}: {}", k, v);
    }
}

fn recover(arc: StaticArc<SharedData>) -> HashMap<String, i32> {
    let mut opt_arc = Some(arc);

    loop {
        let result = opt_arc
            .take()
            .unwrap()
            .try_into_inner_recover();

        match result {
            Ok(shared) => return shared.data.into_inner().unwrap(),
            Err(arc) => opt_arc = Some(arc),
        }
    }
}
