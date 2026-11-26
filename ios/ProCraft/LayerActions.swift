import CoreGraphics
import Foundation

/// What can be done to the layers, each read again afterwards. Those changing pixels have their
/// thumbnails read again with the layers.
extension CanvasModel {
    /// Whether layers are picked to act on with the current one.
    var anyPicked: Bool {
        layers.contains { $0.picked }
    }

    /// Layer `l`'s options menu, as the core lists it.
    func layerOptions(_ l: LayerInfo) -> [LayerOptionRow] {
        session.layerOptions(id: l.id)
    }

    /// Closed group `group`'s layers, top first, as the panel would list them open: a group closed
    /// inside it keeps its own layers out.
    func shownInside(_ group: LayerInfo) -> [LayerInfo] {
        let byId = Dictionary(layers.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return layers.filter { l in
            // the groups it is in, innermost first (a mask's, its layer's)
            var at = l.kind == .mask ? byId[l.parent ?? 0]?.parent : l.parent
            var steps = 0
            while let id = at, steps <= layers.count {
                if id == group.id {
                    return true
                }
                if byId[id]?.collapsed == true {
                    return false
                }
                at = byId[id]?.parent
                steps += 1
            }
            return false
        }
    }

    /// The rows a drag of `l` takes along, it among them (the shared core decides).
    func movingWith(_ l: LayerInfo) -> Set<UInt64> {
        Set(session.movingWith(id: l.id))
    }

    /// Where `l`, dragged with what goes with it, lands if let go with its middle at `y`: `rows`
    /// are the panel's rows where they are on the screen, `held` the row held over a moment, if
    /// any (the shared core decides).
    func rowDrop(_ l: LayerInfo, rows: [UInt64: CGRect], y: CGFloat, held: UInt64?) -> RowDrop {
        let shown = rows.map { ShownRow(id: $0.key, top: Float($0.value.minY), bottom: Float($0.value.maxY)) }
        return session.rowDrop(id: l.id, rows: shown, y: Float(y), held: held)
    }

    /// Moves `l` (with what goes with it) to `place`, as `rowDrop` placed it; a chosen layer with
    /// others picked takes them along, in their order.
    func dropRow(_ l: LayerInfo, _ place: RowDrop) {
        _ = session.dropRow(id: l.id, place: place)
        refresh()
    }

    func setLayerOpacity(_ l: LayerInfo, _ value: Float, done: Bool) {
        _ = session.setLayerOpacity(id: l.id, opacity: value, done: done)
        refresh()
    }

    func setBlend(_ l: LayerInfo, _ blend: UInt32) {
        _ = session.setLayerBlend(id: l.id, blend: blend)
        refresh()
    }

    /// The blend list moving onto `blend`: it shows on `l` at once, the list's whole movement one
    /// undo step.
    func previewBlend(_ l: LayerInfo, _ blend: UInt32) {
        if session.previewLayerBlend(id: l.id, blend: blend) {
            refresh()
        }
    }

    /// The blend list come to rest, or closed: what it showed as it moved is done.
    func finishBlend() {
        if session.finishLayerBlend() {
            refresh()
        }
    }

    func toggleAlphaLock(_ l: LayerInfo) {
        _ = session.setLayerAlphaLock(id: l.id, on: !l.alphaLock)
        refresh()
    }

    func toggleClipped(_ l: LayerInfo) {
        _ = session.setLayerClipped(id: l.id, on: !l.clipped)
        refresh()
    }

    func toggleReference(_ l: LayerInfo) {
        _ = session.setLayerReference(id: l.id, on: !l.reference)
        refresh()
    }

    /// Layer `l` shown alone, or, already alone, every layer again; a message says which (the
    /// core's words). The other rows' boxes show them hidden while it is alone.
    func toggleSolo(_ l: LayerInfo) {
        if let said = session.holdVisibility(id: l.id) {
            showNotice(said)
        }
        refresh()
    }

    func rename(_ l: LayerInfo, _ name: String) {
        _ = session.renameLayer(id: l.id, name: name)
        refresh()
    }

    func duplicate(_ l: LayerInfo) {
        if session.duplicateLayer(id: l.id) == nil {
            noRoomForLayers()
        }
        refresh()
    }

    /// Deletes `l`, asking first when the meshes of a model's texture set share it.
    func deleteAsking(_ l: LayerInfo) {
        guard scene.layersShared, !l.locked else {
            delete(l)
            return
        }
        question = Question(
            title: "Delete shared layer?", message: "\u{201C}\(l.name)\u{201D} is on every mesh of this texture set."
        ) { [weak self] in self?.delete(l) }
    }

    /// Delete on a locked layer's row is told about rather than done (as measured).
    func delete(_ l: LayerInfo) {
        if l.locked {
            lockedDeleteTold = true
            return
        }
        _ = session.deleteLayer(id: l.id)
        refresh()
    }

    func clear(_ l: LayerInfo) {
        _ = session.selectLayer(id: l.id)
        session.clearLayer()
        refresh()
    }

    func addMask(_ l: LayerInfo) {
        if session.addMask(id: l.id) == nil, !hasMask(l) {
            noRoomForLayers()
        }
        refresh()
    }

    func mergeDown(_ l: LayerInfo) {
        _ = session.mergeDown(id: l.id)
        refresh()
    }

    func combineDown(_ l: LayerInfo) {
        _ = session.combineDown(id: l.id)
        refresh()
    }

    func mergeMask(_ l: LayerInfo) {
        _ = session.mergeMask(id: l.id)
        refresh()
    }

    func ungroup(_ l: LayerInfo) {
        _ = session.ungroup(id: l.id)
        refresh()
    }

    func flattenGroup(_ l: LayerInfo) {
        _ = session.flattenGroup(id: l.id)
        refresh()
    }

    func selectLayerContents(_ l: LayerInfo) {
        _ = session.selectLayerContents(id: l.id)
        refresh()
    }

    func fillLayer(_ l: LayerInfo) {
        _ = session.fillLayer(id: l.id)
        refresh()
    }

    func invertLayer(_ l: LayerInfo) {
        _ = session.invertLayer(id: l.id)
        refresh()
    }

    func setGuideAssisted(_ on: Bool) {
        guide.setAssisted(on)
    }

    func rasterize(_ l: LayerInfo) {
        _ = session.rasterizeLayer(id: l.id)
        refresh()
        showNotice(l.text ? "Text rasterized" : "Layer rasterized", warning: true)
    }

    /// A plugin's layer `l`: the strokes the selection takes (every one with nothing selected)
    /// given the paint colour (`colour`), or the paint brush with its size and opacity, as strokes.
    func restyleStrokes(_ l: LayerInfo, colour: Bool) {
        _ = session.selectLayer(id: l.id)
        _ = session.restylePluginItems(colour: colour, brush: !colour, size: !colour, opacity: !colour)
        refresh()
    }

    /// The kinds of layer the plugins on add, for the Layers panel's plus.
    func layerKinds() -> [PluginKind] {
        PluginsModel.shared.store.layerKinds()
    }

    /// A new layer of a kind a plugin adds (`kind`, as the plugin names it), over the current one.
    func addPluginLayer(_ kind: String) {
        if session.addPluginLayer(kind: kind) == 0 {
            noRoomForLayers()
        }
        refresh()
    }

    /// The actions the plugins on list in the Layers panel's `place`: the layer's options (after
    /// the app's own) or rows under the background's.
    func panelPluginActions(_ place: PluginPlace) -> [PluginAction] {
        // asked again as the plugins change
        _ = PluginsModel.shared.changes
        return PluginsModel.shared.store.actionsIn(place: place)
    }

    /// The filter layers the plugins on offer (interface 17): the plus lists them.
    func layerFilters() -> [PluginLayerFilter] {
        PluginsModel.shared.store.layerFilters()
    }

    /// A new filter layer running filter `id` over the current layer, its settings opened at once.
    func addFilterLayer(_ id: String) {
        if session.addFilterLayer(id: id) == 0 {
            noRoomForLayers()
            refresh()
            return
        }
        refresh()
        startFilterLayerSettings()
    }

    // MARK: the picked layers

    /// Picks layer `l` to act on with the current one, or unpicks it.
    func pickLayer(_ l: LayerInfo) {
        _ = session.pickLayer(id: l.id)
        refresh()
    }

    func pickAllLayers() {
        _ = session.pickAllLayers()
        refresh()
    }

    func groupPicked() {
        _ = session.groupPicked()
        refresh()
    }

    func mergePicked() {
        _ = session.mergePicked()
        refresh()
    }

    /// Two rows pinched together: layers `a` and `b` and every one between merge into the lowest.
    func mergeBetween(_ a: UInt64, _ b: UInt64) {
        _ = session.mergeBetween(a: a, b: b)
        refresh()
    }

    func duplicatePicked() {
        if !session.duplicatePicked() {
            noRoomForLayers()
        }
        refresh()
    }

    func deletePicked() {
        _ = session.deletePicked()
        refresh()
    }

    /// Whether the chosen layers include a locked one (Delete then removes nothing).
    func chosenLocked() -> Bool {
        session.chosenLocked()
    }

    /// The current and picked layers each as a PNG, handed to the share sheet.
    func shareChosenLayers() {
        let session = session
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let folder = Sharing.exportsDir.appendingPathComponent("layers")
            try? FileManager.default.removeItem(at: folder)
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let files = (try? session.exportChosenPngs(dir: folder.path)) ?? []
            DispatchQueue.main.async {
                if files.isEmpty {
                    self?.showNotice("The layers couldn't be exported.", warning: true)
                } else {
                    Sharing.share(files.map { URL(fileURLWithPath: $0) })
                }
            }
        }
    }
}
