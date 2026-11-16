#![deny(warnings, clippy::pedantic, clippy::all, rust_2018_idioms)]

use nsh_core::prelude as nsh;

#[nsh::native_module("skull/native")]
fn native(registry: &mut nsh::Registry) {
    registry.register::<App>();
}

#[nsh::component]
pub fn App() -> impl nsh::IntoElement {
    use nsh::{Interactive as _, ParentElement as _, Styled as _};

    let count = nsh::signal(0_u32);
    nsh::view()
        .flex_1()
        .bg(nsh_base::theme().background)
        .text_color(nsh_base::theme().foreground)
        .child(
            nsh::view().flex_1().p_safe_area().child(
                nsh::view()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        nsh::text(move || format!("Count: {}", count.get()))
                            .text_size(nsh::px(28.0)),
                    )
                    .child(
                        nsh::view()
                            .px_5()
                            .py_3()
                            .rounded(nsh::px(10.0))
                            .bg(nsh_base::theme().primary)
                            .active(|s| s.opacity(0.6))
                            .on_press(move || count.update(|value| *value += 1))
                            .child(
                                nsh::text("Tap")
                                    .text_size(nsh::px(17.0))
                                    .text_color(nsh_base::theme().primary_foreground),
                            ),
                    ),
            ),
        )
}
