import SwiftUI
import UIKit

/// A name edited where it is shown: focused with the keyboard up; done on the keyboard's return or
/// when focus leaves, once. `onChange` hears it as it is typed, where one is given.
struct InlineName: View {
    let name: String
    let color: Color
    let fontSize: CGFloat
    var weight: Font.Weight = .regular
    let onDone: (String) -> Void
    var onChange: ((String) -> Void)?

    @State private var value = ""
    @State private var finished = false
    @FocusState private var focused: Bool

    var body: some View {
        TextField(text: $value) { EmptyView() }
            .font(.system(size: fontSize, weight: weight))
            .foregroundStyle(color)
            .tint(color)
            .autocorrectionDisabled()
            .submitLabel(.done)
            .focused($focused)
            .onSubmit(finish)
            .onChange(of: value) { _, now in
                if !finished {
                    onChange?(now)
                }
            }
            .onChange(of: focused) { was, now in
                if was, !now {
                    finish()
                }
            }
            .onAppear {
                value = name
                focused = true
                // the whole name chosen, so typing replaces it
                DispatchQueue.main.async {
                    UIApplication.shared.sendAction(#selector(UIResponder.selectAll(_:)), to: nil, from: nil, for: nil)
                }
            }
    }

    private func finish() {
        if !finished {
            finished = true
            onDone(value)
        }
    }
}
