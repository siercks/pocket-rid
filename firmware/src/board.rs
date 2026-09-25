use esp_hal::gpio::Output;

/// Every panel pin, in the section 2.3 start levels. Built by `main`, consumed by `ui_task`.
pub struct PanelPins {
    pub pwr: Output<'static>,
    pub rd: Output<'static>,
    pub cs: Output<'static>,
    pub dc: Output<'static>,
    pub wr: Output<'static>,
    pub rst: Output<'static>,
    pub bl: Output<'static>,
    pub d: [Output<'static>; 8],
}

/// Dropping any of these would float the pin and blank or unpower the panel.
pub struct PanelKeepAlive {
    pub _pwr: Output<'static>,
    pub _rd: Output<'static>,
    pub _cs: Output<'static>,
    pub _bl: Output<'static>,
}
