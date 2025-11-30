use data::configdata::{ConfigContest, SedeEntry};
use leptos::{html::Input, prelude::*};
use leptos_router::{hooks::use_query_map, params::ParamsMap};


#[component]
fn Sede(sede: SedeEntry, query: Memo<ParamsMap>) -> impl IntoView {
    let name = sede.name.clone();

    move || {
        let mut params = query.get();
        let name = name.clone();
        params.replace("sede".to_string(), name.clone());
        view! {
            <a href=params.to_query_string()  role="option"> {name} </a>
        }
    }
}

#[component]
pub fn Navigation(config_contest: LocalResource<ConfigContest>) -> impl IntoView {
    let query = use_query_map();
    let (is_open, set_is_open) = signal(false);
    let (filter, set_filter) = signal(String::new());
    let search_input = NodeRef::<Input>::new();

    let _ = window_event_listener(leptos::ev::click, move |ev| {
        let target = event_target::<web_sys::HtmlElement>(&ev);
        if target.closest(".sede-dropdown").ok().flatten().is_none() {
            set_is_open.set(false);
        }
    });

    move || {
        view! {
            <div class="sede-dropdown">
                <button
                    class="sede-dropdown-button"
                    id="sedeDropdownBtn"
                    aria-haspopup="listbox"
                    aria-expanded=move || is_open.get().to_string()
                    on:click=move |_| {
                        let open = !is_open.get();
                        set_is_open.set(open);
                        if open {
                            set_filter.set(String::new());
                            if let Some(input) = search_input.get() {
                                let _ = input.focus();
                            }
                        }
                    }
                >
                    Alterar a sede
                </button>
                <div
                    class="sede-dropdown-content"
                    class:show=move || is_open.get()
                    id="sedeDropdownContent"
                    role="listbox"
                    tabindex="-1"
                    aria-label="Lista de sedes"
                >
                    <input
                        type="text"
                        id="sedeSearchInput"
                        placeholder="Filtrar sedes..."
                        aria-label="Filtro de sedes"
                        node_ref=search_input
                        prop:value=move || filter.get()
                        on:input=move |ev| set_filter.set(event_target_value(&ev))
                    />

                    <Suspense fallback=||view! {<p> Loading... </p> }>
                        {move || {
                            config_contest.with(|config| config.as_ref().map(|contest| {
                                contest
                                .sedes.iter().flatten()
                                .cloned()
                                .filter(|sede| {
                                    let filter_text = filter.get().to_lowercase();
                                    filter_text.is_empty() || sede.name.to_lowercase().contains(&filter_text)
                                })
                                .map(move |sede| {
                                    view! {<Sede sede query />}
                                })
                                .collect_view()
                            }))
                        }}
                    </Suspense>
                </div>
            </div>
        }
    }
}
