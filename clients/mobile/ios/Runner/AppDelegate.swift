import Flutter
import CryptoKit
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate {
  private let ambientPlaybackBridge = AmbientPlaybackBridge()

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    GeneratedPluginRegistrant.register(with: self)
    if let controller = window?.rootViewController as? FlutterViewController {
      if let registrar = registrar(forPlugin: "DuskcueAmbientPlayback") {
        ambientPlaybackBridge.register(messenger: controller.binaryMessenger, registrar: registrar)
      }
      FlutterMethodChannel(
        name: "duskcue/mobile_storage",
        binaryMessenger: controller.binaryMessenger
      ).setMethodCallHandler { [weak self] call, result in
        self?.handleStorageCall(call: call, result: result)
      }
    }
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  private func handleStorageCall(call: FlutterMethodCall, result: @escaping FlutterResult) {
    do {
      let args = call.arguments as? [String: Any] ?? [:]
      switch call.method {
      case "prepareDownloadScope":
        let scopeKey = args["scope_key"] as? String ?? ""
        result(locationMap(for: try scopeDirectory(scopeKey: scopeKey, create: true)))
      case "prepareDownloadPackage":
        let scopeKey = args["scope_key"] as? String ?? ""
        let packageKey = args["package_key"] as? String ?? ""
        result(locationMap(for: try packageDirectory(scopeKey: scopeKey, packageKey: packageKey, create: true)))
      case "deleteDownloadPackage":
        let scopeKey = args["scope_key"] as? String ?? ""
        let packageKey = args["package_key"] as? String ?? ""
        let url = try packageDirectory(scopeKey: scopeKey, packageKey: packageKey, create: false)
        try? FileManager.default.removeItem(at: url)
        result(nil)
      case "deleteDownloadScope":
        let scopeKey = args["scope_key"] as? String ?? ""
        let url = try scopeDirectory(scopeKey: scopeKey, create: false)
        try? FileManager.default.removeItem(at: url)
        result(nil)
      case "deleteAllDownloads":
        try? FileManager.default.removeItem(at: downloadsRoot())
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    } catch {
      result(FlutterError(code: "download_storage_failed", message: error.localizedDescription, details: nil))
    }
  }

  private func downloadsRoot() throws -> URL {
    let support = try FileManager.default.url(
      for: .applicationSupportDirectory,
      in: .userDomainMask,
      appropriateFor: nil,
      create: true
    )
    let root = support.appendingPathComponent("DuskcueDownloads", isDirectory: true)
    try protectDirectory(root)
    return root
  }

  private func scopeDirectory(scopeKey: String, create: Bool) throws -> URL {
    let url = try downloadsRoot().appendingPathComponent(digest(scopeKey), isDirectory: true)
    if create {
      try protectDirectory(url)
    }
    return url
  }

  private func packageDirectory(scopeKey: String, packageKey: String, create: Bool) throws -> URL {
    let url = try scopeDirectory(scopeKey: scopeKey, create: create).appendingPathComponent(digest(packageKey), isDirectory: true)
    if create {
      try protectDirectory(url)
    }
    return url
  }

  private func protectDirectory(_ url: URL) throws {
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    var mutableUrl = url
    try mutableUrl.setResourceValues(values)
    try FileManager.default.setAttributes(
      [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
      ofItemAtPath: url.path
    )
  }

  private func locationMap(for url: URL) -> [String: Any] {
    return [
      "path": url.path,
      "platform": "ios",
      "backup_excluded": true,
      "protection": "complete_until_first_user_authentication"
    ]
  }

  private func digest(_ value: String) -> String {
    let data = Data(value.utf8)
    let hash = SHA256.hash(data: data)
    return hash.map { String(format: "%02x", $0) }.joined()
  }
}
