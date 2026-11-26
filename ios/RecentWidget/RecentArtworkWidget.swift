import SwiftUI
import UIKit
import WidgetKit

// The home screen's Recent artwork widget: the picture of the artwork worked on last filling it
// (the app's mark while there is none); a tap opens the app at that artwork. The app keeps the
// picture in the folder it shares with the widget (ProCraft/RecentArtwork.swift) and asks the
// widget to show it again whenever it changes. Built only with PROCRAFT_WIDGET=YES (ios/widget.yml).

/// The app group the app and the widget share: as the app's `RecentArtwork.group`.
private let appGroup = "group.io.github.artcraftandroid.procraft"
/// The picture's file in that group's folder: as the app's `RecentArtwork.pictureName`.
private let pictureName = "recent.png"
/// What a tap on the widget opens the app with: as the app's `RecentArtwork.isOpenRecent`.
private let openRecent = URL(string: "procraft://recent")
/// Behind the picture, and the mark: the gallery's own dark.
private let backdrop = Color(red: 0x21 / 255.0, green: 0x21 / 255.0, blue: 0x21 / 255.0)

/// What the widget shows: the picture kept, if there is one.
struct RecentArtworkEntry: TimelineEntry {
    let date: Date
    let picture: UIImage?
}

/// The widget's one entry, read again only when the app asks (it reloads the widget as the
/// artwork worked on last changes).
struct RecentArtworkProvider: TimelineProvider {
    func placeholder(in context: Context) -> RecentArtworkEntry {
        RecentArtworkEntry(date: Date(), picture: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (RecentArtworkEntry) -> Void) {
        completion(entry())
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<RecentArtworkEntry>) -> Void) {
        completion(Timeline(entries: [entry()], policy: .never))
    }

    private func entry() -> RecentArtworkEntry {
        let folder = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
        let picture = folder.flatMap { UIImage(contentsOfFile: $0.appendingPathComponent(pictureName).path) }
        return RecentArtworkEntry(date: Date(), picture: picture)
    }
}

/// The picture filling the widget, or the app's mark in the middle of the gallery's dark.
struct RecentArtworkWidgetView: View {
    let entry: RecentArtworkEntry

    var body: some View {
        GeometryReader { place in
            if let picture = entry.picture {
                Image(uiImage: picture)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFill()
                    .frame(width: place.size.width, height: place.size.height)
                    .clipped()
            } else if let mark = UIImage(named: "procraft_mark") {
                let side = min(place.size.width, place.size.height) * 0.5
                Image(uiImage: mark)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .frame(width: side, height: side)
                    .frame(width: place.size.width, height: place.size.height)
            }
        }
        .containerBackground(for: .widget) { backdrop }
        .widgetURL(openRecent)
    }
}

@main
struct RecentArtworkWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "RecentArtwork", provider: RecentArtworkProvider()) { entry in
            RecentArtworkWidgetView(entry: entry)
        }
        .configurationDisplayName("Recent artwork")
        .description("Opens ProCraft at the artwork you worked on last.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge, .systemExtraLarge])
        .contentMarginsDisabled()
    }
}
