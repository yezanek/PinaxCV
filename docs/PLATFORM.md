# PinaxCV Platform Strategy

## Linux and Windows — v1

These are the only supported desktop platforms initially.

Platform-specific concerns include:

- application data directories;
- keychain/credential store;
- file dialogs;
- update mechanisms;
- packaging;
- native webview behavior.

Keep these concerns behind adapters where practical.

## macOS — future

Add only when demand justifies the testing, signing, packaging, and support cost. Reuse the same Rust domain and application layers.

## SaaS — future

A hosted product is a deployment model, not a replacement for the desktop product. The standalone desktop application remains useful without the hosted service.
