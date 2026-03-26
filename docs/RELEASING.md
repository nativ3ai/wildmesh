# Releasing WildMesh

This is the release path for source, GitHub releases, and Homebrew packaging.

## 1. Update versions

Update release version in:

- `Cargo.toml`
- `plugin.yaml`
- `pyproject.toml`
- `README.md` install examples if needed
- `Formula/wildmesh.rb`

## 2. Verify locally

```bash
cargo fmt
cargo build
cargo test
pytest tests/test_hermes_plugin.py
```

## 3. Commit and tag source

```bash
git add .
git commit -m "release: vX.Y.Z"
git push origin main
git tag vX.Y.Z
git push origin vX.Y.Z
```

## 4. Create GitHub release

```bash
gh release create vX.Y.Z --repo nativ3ai/wildmesh --title "WildMesh vX.Y.Z" --notes "..."
```

## 5. Update tap formula source tarball

Point formula to:

- `https://github.com/nativ3ai/wildmesh/archive/refs/tags/vX.Y.Z.tar.gz`

Update source sha in:

- source repo `Formula/wildmesh.rb`
- tap repo `Formula/wildmesh.rb`

## 6. Build bottle

From the tap checkout:

```bash
brew uninstall --force wildmesh || true
HOMEBREW_NO_INSTALL_FROM_API=1 HOMEBREW_NO_AUTO_UPDATE=1 brew install --build-bottle nativ3ai/wildmesh/wildmesh
brew bottle --json --root-url=https://github.com/nativ3ai/wildmesh/releases/download/vX.Y.Z wildmesh
```

Update bottle sha in both formulas.

## 7. Upload bottle assets

Upload:

- `wildmesh--X.Y.Z.arm64_tahoe.bottle.tar.gz`
- `wildmesh-X.Y.Z.arm64_tahoe.bottle.tar.gz`
- `wildmesh--X.Y.Z.arm64_tahoe.bottle.json`

to the GitHub release.

## 8. Push tap update

Commit only the formula update in the tap repo:

```bash
git add Formula/wildmesh.rb
git commit -m "wildmesh X.Y.Z"
git push origin main
```

## 9. Final public verification

```bash
brew reinstall --force-bottle nativ3ai/wildmesh/wildmesh
wildmesh --version
wildmesh setup --agent-label smoke --with-hermes false --launch-agent false
wildmesh profile --json
```

If Hermes integration changed:

```bash
wildmesh install-hermes-plugin --hermes-home ~/.hermes
```

## 10. Release bar

Do not call a release complete until:

- source repo is pushed
- release exists
- bottle assets exist
- tap formula is pushed
- public install works
