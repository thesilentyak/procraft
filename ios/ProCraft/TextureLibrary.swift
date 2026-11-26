import SwiftUI
import UIKit
import UniformTypeIdentifiers

private let texturesVeil = Color(argb: 0x8000_0000)
private var texturesSheet: Color { Color(argb: Look.shared.light ? 0xFFF2_F2F7 : 0xFF1C_1C1E) }
private var texturesTitle: Color { Color(argb: Look.shared.light ? 0xFF3A_3A3A : 0xFFC1_C1C1) }
private let texturesDim = Color(argb: 0xFF86_8686)
private var texturesEdge: Color { Color(argb: Look.shared.light ? 0xFFC8_C8C8 : 0xFF5A_5A5A) }

/// A texture control whose Textures sheet is open: its key, and what tells of a picture that
/// can't be used.
struct TextureAsk {
    let key: String
    let told: (String) -> Void
}

/// The texture control kept under `key`, as the list has it now.
private func textureControl(_ key: String, in plugins: [PluginInfo]) -> PluginTexture? {
    for plugin in plugins {
        for control in plugin.controls {
            if case let .texture(texture) = control, texture.key == key {
                return texture
            }
        }
    }
    return nil
}

/// What the picture picked for the plugin's file control `key` is called, if one is.
private func pictureName(_ key: String, in plugins: [PluginInfo]) -> String? {
    for plugin in plugins {
        for control in plugin.controls {
            if case let .file(file) = control, file.key == key {
                return file.chosen
            }
        }
    }
    return nil
}

/// The texture `control` has chosen, in words: a grain's name, the name of a picture of the
/// plugin's, or None. A grain says nothing until the grains' names are read.
func textureName(_ control: PluginTexture, grains: [String]?, plugins: [PluginInfo]) -> String {
    let v = Int(control.value)
    if v > 0 {
        guard let grains else { return "" }
        return v < grains.count ? grains[v] : "None"
    }
    let at = -1 - v
    if v < 0, at < control.own.count, let name = pictureName(control.own[at], in: plugins) {
        return name
    }
    return "None"
}

/// A plugin's picture as the core gives it (straight RGBA rows), for a tile.
private func pictureImage(_ picture: PluginPicture) -> UIImage? {
    let width = Int(picture.width)
    let height = Int(picture.height)
    guard width > 0, height > 0, picture.pixels.count == width * height * 4,
          let provider = CGDataProvider(data: Data(picture.pixels) as CFData),
          let image = CGImage(
              width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
              space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
              bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
              provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent
          ) else { return nil }
    return UIImage(cgImage: image)
}

/// A plugin's texture control (interface 15), as its card shows it: its label, the texture chosen
/// in blue; a tap opens the Textures sheet.
struct PluginTextureRow: View {
    let texture: PluginTexture
    let told: (String) -> Void

    /// The grains' names, once read.
    @State private var grains: [String]?

    var body: some View {
        HStack(spacing: 0) {
            Text(verbatim: texture.label)
                .font(.system(size: 17))
                .foregroundStyle(Pc.actionsRow)
                .lineLimit(1)
            Spacer(minLength: 0)
            Text(verbatim: textureName(texture, grains: grains, plugins: PluginsModel.shared.plugins))
                .font(.system(size: 17))
                .foregroundStyle(Pc.accent)
                .lineLimit(1)
                .padding(.leading, 12)
        }
        .padding(.horizontal, 16)
        .frame(height: 44)
        .contentShape(Rectangle())
        .onTapGesture { PluginsModel.shared.texturing = TextureAsk(key: texture.key, told: told) }
        .task {
            grains = await Task.detached(priority: .userInitiated) {
                SourceImages.library(grain: true).map(\.name)
            }.value
        }
    }
}

/// A tile on the Textures sheet.
private struct TextureTile {
    let name: String
    let image: UIImage?
    /// What choosing it sets the control to; none for the tile that adds a picture.
    let value: Int32?
    /// The file control whose picture it is, for one of the plugin's own.
    let file: String?
}

/// The Textures sheet a texture control opens, over the whole window as the Source Library is:
/// the app's grains, then the pictures picked for the plugin's own file controls it offers, then a
/// tile that picks another into the first of them with none (and chooses it). A tap chooses one
/// and closes it; holding one of the plugin's pictures offers to delete it.
struct TextureLibrary: View {
    let ask: TextureAsk

    @State private var plugins = PluginsModel.shared
    @State private var grains: [LibrarySource] = []
    /// The plugin's pictures, small, by their file controls' keys.
    @State private var pictures: [String: UIImage] = [:]
    @State private var picking = false
    /// One of the plugin's pictures held, waiting on the alert that asks first.
    @State private var deleting: String?

    /// The control as it is now: a picture added or deleted changes the plugin's list.
    private var control: PluginTexture? {
        textureControl(ask.key, in: plugins.plugins)
    }

    /// The plugin's file controls the control offers that have a picture, and what each is called.
    private func own(_ control: PluginTexture) -> [(file: String, name: String)] {
        control.own.compactMap { file in
            pictureName(file, in: plugins.plugins).map { (file: file, name: $0) }
        }
    }

    /// The first of those with no picture yet, by its place.
    private func firstFree(_ control: PluginTexture) -> Int? {
        control.own.firstIndex { pictureName($0, in: plugins.plugins) == nil }
    }

    private func tiles(_ control: PluginTexture) -> [TextureTile] {
        var tiles = grains.enumerated().map {
            TextureTile(name: $0.element.name, image: $0.element.image, value: Int32($0.offset), file: nil)
        }
        for (at, file) in control.own.enumerated() {
            if let name = pictureName(file, in: plugins.plugins) {
                tiles.append(TextureTile(name: name, image: pictures[file], value: Int32(-1 - at), file: file))
            }
        }
        if firstFree(control) != nil {
            tiles.append(TextureTile(name: "Add a picture", image: nil, value: nil, file: nil))
        }
        return tiles
    }

    private func close() {
        plugins.texturing = nil
    }

    private func cell(_ tile: TextureTile, _ control: PluginTexture) -> some View {
        let shape = RoundedRectangle(cornerRadius: 8, style: .circular)
        let chosen = tile.value != nil && tile.value == control.value
        return VStack(alignment: .leading, spacing: 0) {
            ZStack {
                Color.black
                if let image = tile.image {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: 147, height: 146)
                        .clipped()
                } else if tile.value == nil {
                    Text(verbatim: "+")
                        .font(.system(size: 48))
                        .foregroundStyle(Pc.accent)
                }
            }
            .frame(width: 147, height: 146)
            .clipShape(shape)
            .overlay(shape.strokeBorder(chosen ? Pc.accent : texturesEdge, lineWidth: chosen ? 3 : 1))
            Text(verbatim: tile.name)
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(texturesTitle)
                .lineLimit(1)
                .frame(height: 21, alignment: .top)
                .padding(.leading, 3)
                .padding(.top, 5)
        }
        .frame(width: 147, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture {
            if let value = tile.value {
                plugins.setTexture(control.key, value)
                close()
            } else {
                picking = true
            }
        }
        .onLongPressGesture(minimumDuration: 0.5, maximumDistance: 10) {
            if let file = tile.file {
                deleting = file
            }
        }
    }

    private func sheet(_ control: PluginTexture) -> some View {
        let tiles = tiles(control)
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 0) {
                Text(verbatim: "Textures")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(texturesTitle)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Text(verbatim: "Cancel")
                    .font(.system(size: 17))
                    .foregroundStyle(texturesDim)
                    .lineLimit(1)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: close)
            }
            .frame(height: 39)
            .padding(.horizontal, 19)
            .padding(.top, 40)
            ScrollView {
                LazyVGrid(columns: Array(repeating: GridItem(.fixed(147), spacing: 53), count: 3), spacing: 18) {
                    ForEach(tiles.indices, id: \.self) { k in
                        cell(tiles[k], control)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 31)
                .padding(.bottom, 24)
            }
            .scrollIndicators(.hidden)
        }
        .frame(width: 580, height: 575)
        .background(texturesSheet)
        .clipShape(RoundedRectangle(cornerRadius: 26, style: .circular))
        .offset(y: 74)
    }

    var body: some View {
        let control = self.control
        // the pictures read again as one is added or deleted
        let shown = control.map { c in own(c).map { $0.file + "\n" + $0.name } } ?? []
        ZStack(alignment: .top) {
            // over everything, what is under it dimmed
            texturesVeil
                .contentShape(Rectangle())
            if let control {
                sheet(control)
            }
            AlertView(
                content: deleting.map { _ in
                    AlertContent(
                        title: "Delete this picture?",
                        message: "It goes from the plugin's textures.",
                        action: "Delete",
                        icon: PcIcons.trash
                    )
                },
                onCancel: { deleting = nil },
                onAction: {
                    if let file = deleting {
                        plugins.clearFile(file)
                    }
                    deleting = nil
                }
            )
        }
        // a plugin turned off or removed takes its control, and the sheet, with it
        .onChange(of: control == nil, initial: true) { _, gone in
            if gone {
                close()
            }
        }
        .task {
            grains = await Task.detached(priority: .userInitiated) {
                SourceImages.library(grain: true)
            }.value
        }
        .task(id: shown) {
            let store = plugins.store
            let files = control.map { c in own(c).map { $0.file } } ?? []
            pictures = await Task.detached(priority: .userInitiated) { () -> [String: UIImage] in
                var made: [String: UIImage] = [:]
                for file in files {
                    if let picture = store.pictureThumbnail(key: file, side: 256), let image = pictureImage(picture) {
                        made[file] = image
                    }
                }
                return made
            }.value
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.image]) { result in
            guard case let .success(url) = result, let now = self.control, let at = firstFree(now) else { return }
            plugins.pickTexture(now.key, file: now.own[at], value: Int32(-1 - at), from: url, told: ask.told)
            close()
        }
    }
}
