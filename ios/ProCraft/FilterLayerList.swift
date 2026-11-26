import SwiftUI

/// The list the Layers panel's plus opens for a filter layer (interface 17): the filters the
/// plugins on offer under their headings, in the order they list them. A tap picks one; Cancel
/// closes it.
struct FilterLayerList: View {
    let filters: [PluginLayerFilter]
    let onPick: (String) -> Void
    let onClose: () -> Void

    /// The filters under their headings, each heading once, where its first filter comes.
    private var groups: [(String, [PluginLayerFilter])] {
        var out: [(String, [PluginLayerFilter])] = []
        for f in filters {
            if let at = out.firstIndex(where: { $0.0 == f.group }) {
                out[at].1.append(f)
            } else {
                out.append((f.group, [f]))
            }
        }
        return out
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(groups, id: \.0) { group in
                    Section(group.0) {
                        ForEach(group.1, id: \.id) { f in
                            Button(f.label) { onPick(f.id) }
                        }
                    }
                }
            }
            .navigationTitle("Filter Layer")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", action: onClose)
                }
            }
        }
    }
}
