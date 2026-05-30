import Foundation

/// A sample reference type.
class Foo: NSObject {
    var name: String = ""
    let id: Int = 0

    func greet(_ msg: String) -> String {
        return msg
    }
}

struct Point {
    let x: Int
    let y: Int
}

protocol Drawable {
    func draw()
}

enum Color {
    case red, green, blue
}

func topLevel() {}
