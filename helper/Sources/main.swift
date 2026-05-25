import Foundation
signal(SIGTERM) { _ in exit(0) }
signal(SIGINT)  { _ in exit(0) }
RunLoop.main.run()
