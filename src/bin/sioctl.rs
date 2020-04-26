use sioctl::*;

fn main() {
    //let p = |parameter| println!("parameter: {:?}", parameter);
    //let v = |value| println!("value: {:?}", value);
    let s = Sioctl::new();

    println!("initial");
    for control in s.controls() {
        println!("{:?}", control);
    }

    println!("watching...");
    s.watch(|control| println!("{:?}", control));

    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).unwrap();
}

