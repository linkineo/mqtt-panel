import Foundation
import CryptoKit
let args = CommandLine.arguments
let keyID = args[1]; let issuer = args.count > 2 ? args[2] : ""
let pem = try! String(contentsOfFile: NSString(string: "~/.appstoreconnect/private_keys/AuthKey_\(keyID).p8").expandingTildeInPath, encoding: .utf8)
let key = try! P256.Signing.PrivateKey(pemRepresentation: pem)
func b64(_ d: Data) -> String { d.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "") }
let now = Int(Date().timeIntervalSince1970)
let header: [String: Any] = ["alg": "ES256", "kid": keyID, "typ": "JWT"]
var payload: [String: Any] = ["aud": "appstoreconnect-v1", "iat": now, "exp": now + 1200]
if issuer.isEmpty { payload["sub"] = "user" } else { payload["iss"] = issuer }
let h = b64(try! JSONSerialization.data(withJSONObject: header)); let p = b64(try! JSONSerialization.data(withJSONObject: payload))
let sig = try! key.signature(for: Data("\(h).\(p)".utf8))
print("\(h).\(p).\(b64(sig.rawRepresentation))")
