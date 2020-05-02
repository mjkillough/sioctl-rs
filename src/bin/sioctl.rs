use sioctl::Sioctl;

fn main() {
    let s = Sioctl::new();

    println!("Initial state of controls:");
    for control in s.controls() {
        println!("{:?}", control);
    }

    println!("");

    println!("Watching for changes (press Enter to exit):");
    let mut watcher = s.watch(|control| println!("{:?}", control));

    // Wait for Enter:
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).unwrap();

    println!("Shutting down...");
    watcher.join();
    println!("Gracefully shut down. Bye.");
}
