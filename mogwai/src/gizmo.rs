use std::{cell::RefCell, rc::Rc};
pub use wasm_bindgen::{JsCast, JsValue, UnwrapThrowExt};
use web_sys::Node;
pub use web_sys::{Element, Event, EventTarget, HtmlInputElement};

pub use super::utils;
use super::{
    component::{subscriber::Subscriber, Component},
    txrx::{txrx, Receiver, Transmitter},
};

pub mod dom;
pub mod view;
use dom::DomWrapper;

/// A concrete component/widget and all of its pieces.
pub struct Gizmo<T: Component> {
    pub trns: Transmitter<T::ModelMsg>,
    pub recv: Receiver<T::ViewMsg>,

    pub(crate) view: DomWrapper<T::DomNode>,
    pub(crate) state: Rc<RefCell<T>>,
}


impl<T> Gizmo<T>
where
    T: Component + 'static,
    T::ViewMsg: Clone,
    T::DomNode: AsRef<Node> + Clone,
{
    pub fn new(init: T) -> Gizmo<T> {
        let component_var = Rc::new(RefCell::new(init));
        let state = component_var.clone();
        let (tx_out, rx_out) = txrx();
        let (tx_in, rx_in) = txrx();
        let subscriber = Subscriber::new(&tx_in);

        let (tx_view, rx_view) = txrx();
        rx_in.respond(move |msg: &T::ModelMsg| {
            let mut t = state.borrow_mut();
            T::update(&mut t, msg, &tx_view, &subscriber);
        });

        rx_view.respond(move |msg: &T::ViewMsg| {
            let tx_out = tx_out.clone();
            let msg = msg.clone();
            utils::set_immediate(move || tx_out.send(&msg));
        });

        let gizmo = {
            let component = component_var.borrow();
            component.view(tx_in.clone(), rx_out.branch())
        };

        Gizmo {
            trns: tx_in,
            recv: rx_out,
            view: gizmo,
            state: component_var,
        }
    }

    /// A reference to the browser's DomNode.
    ///
    /// # Panics
    /// Only works in the browser. Panics outside of wasm32.
    pub fn dom_ref(&self) -> &T::DomNode {
        if cfg!(target_arch = "wasm32") {
            return self.view.as_ref().unchecked_ref::<T::DomNode>();
        }
        panic!("Gizmo::dom_ref is only available on wasm32")
    }

    pub fn view_ref(&self) -> &DomWrapper<T::DomNode> {
        &self.view
    }

    /// Send model messages into this component from a `Receiver<T::ModelMsg>`.
    /// This is helpful for sending messages to this component from
    /// a parent component.
    pub fn rx_from(self, rx: Receiver<T::ModelMsg>) -> Gizmo<T> {
        rx.forward_map(&self.trns, |msg| msg.clone());
        self
    }

    /// Send view messages from this component into a `Transmitter<T::ViewMsg>`.
    /// This is helpful for sending messages to this component from
    /// a parent component.
    pub fn tx_into(self, tx: &Transmitter<T::ViewMsg>) -> Gizmo<T> {
        self.recv.branch().forward_map(&tx, |msg| msg.clone());
        self
    }

    /// Run and initialize the component with a list of messages.
    /// This is equivalent to calling `run` and `update` with each message.
    pub fn run_init(mut self, msgs: Vec<T::ModelMsg>) -> Result<(), JsValue> {
        msgs.into_iter().for_each(|msg| {
            self.update(&msg);
        });
        self.run()
    }

    /// Run this component forever
    ///
    /// # Panics
    /// Only works in the browser. Panics on compilation targets that are not
    /// wasm32.
    pub fn run(self) -> Result<(), JsValue> {
        if cfg!(target_arch = "wasm32") {
            return self.view.run();
        }
        panic!("Gizmo::run is only available on wasm32")
    }

    /// Update the component with the given message.
    /// This how a parent component communicates down to its child components.
    pub fn update(&mut self, msg: &T::ModelMsg) {
        self.trns.send(msg);
    }

    /// Access the underlying state.
    pub fn with_state<F, N>(&self, f: F) -> N
    where
        F: Fn(&T) -> N,
    {
        let t = self.state.borrow();
        f(&t)
    }
}


impl<T: Component> From<T> for Gizmo<T> {
    fn from(component: T) -> Gizmo<T> {
        Gizmo::new(component)
    }
}


/// The type of function that uses a txrx pair and returns a DomWrapper.
pub type BuilderFn<T, D> = dyn Fn(Transmitter<T>, Receiver<T>) -> DomWrapper<D>;


/// A simple component made from a [BuilderFn].
///
/// Any function that takes a transmitter and receiver of the same type and
/// returns a [DomWrapper] can be made into a component that holds no internal
/// state. It forwards all of its incoming messages to its view.
///
/// ```rust,no_run
/// extern crate mogwai;
/// use mogwai::prelude::*;
///
/// let component: SimpleComponent<(), HtmlElement> = (
///     Box::new(
///         |tx: Transmitter<()>, rx: Receiver<()>| -> DomWrapper<HtmlElement> {
///             dom!{
///                 <button style="pointer" on:click=tx.contra_map(|_| ())>
///                     {("Click me", rx.branch_map(|()| "Clicked!".to_string()))}
///                 </button>
///             }
///         }
///     ) as Box<BuilderFn<(), HtmlElement>>
/// ).into_gizmo();
/// ```
pub type SimpleComponent<T, D> = Gizmo<Box<BuilderFn<T, D>>>;


impl<T, D> Component for Box<BuilderFn<T, D>>
where
    T: Clone + 'static,
    D: JsCast + AsRef<Node> + Clone + 'static,
{
    type ModelMsg = T;
    type ViewMsg = T;
    type DomNode = D;

    fn update(&mut self, msg: &T, tx_view: &Transmitter<T>, _sub: &Subscriber<T>) {
        tx_view.send(msg);
    }

    fn view(&self, tx: Transmitter<T>, rx: Receiver<T>) -> DomWrapper<D> {
        self(tx, rx)
    }
}
