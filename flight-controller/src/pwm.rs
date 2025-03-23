use defmt::info;
use embassy_rp::Peripheral;
use embassy_rp::pwm::{self, ChannelAPin, ChannelBPin, Pwm, PwmError, PwmOutput, SetDutyCycle};

pub struct RawPwm<'a> {
    inner: PwmOutput<'a>,
    /// Number of ticks in the period
    period: u16,
    pub min_percent: f32,
    pub max_percent: f32,
}

impl<'a> RawPwm<'a> {
    pub fn new<T: embassy_rp::pwm::Slice>(
        slice: impl Peripheral<P = T> + 'a,
        pin: impl Peripheral<P = impl ChannelAPin<T>> + 'a,
        duty_cycle_hz: u32,
        divider: u8,
        min_percent: f32,
        max_percent: f32,
    ) -> Self {
        // If we aim for a specific frequency, here is how we can calculate the top value.
        // The top value sets the period of the PWM cycle, so a counter goes from 0 to top and then wraps around to 0.
        // Every such wraparound is one PWM cycle. So here is how we get 50KHz:
        let clock_freq_hz = embassy_rp::clocks::clk_sys_freq();
        let period = (clock_freq_hz / (duty_cycle_hz * divider as u32)) as u16 - 1;
        info!("divider: {}, period: {}", divider, period);

        let mut c = pwm::Config::default();
        c.top = period;
        c.divider = divider.into();

        Self {
            inner: Pwm::new_output_a(slice, pin, c.clone())
                .split()
                .0
                .expect("When just making channel a it should have it"),
            period,
            min_percent,
            max_percent,
        }
    }

    pub fn new_ab<T: embassy_rp::pwm::Slice>(
        slice: impl Peripheral<P = T> + 'a,
        pin_a: impl Peripheral<P = impl ChannelAPin<T>> + 'a,
        pin_b: impl Peripheral<P = impl ChannelBPin<T>> + 'a,
        duty_cycle_hz: u32,
        divider: u8,
        min_percent: f32,
        max_percent: f32,
    ) -> (Self, Self) {
        // If we aim for a specific frequency, here is how we can calculate the top value.
        // The top value sets the period of the PWM cycle, so a counter goes from 0 to top and then wraps around to 0.
        // Every such wraparound is one PWM cycle. So here is how we get 50KHz:
        let clock_freq_hz = embassy_rp::clocks::clk_sys_freq();
        let period = (clock_freq_hz / (duty_cycle_hz * divider as u32)) as u16 - 1;
        info!("divider: {}, period: {}", divider, period);

        let mut c = pwm::Config::default();
        c.top = period;
        c.divider = divider.into();

        let portions = Pwm::new_output_ab(slice, pin_a, pin_b, c.clone()).split();
        (
            Self {
                inner: portions.0.expect("PWM Channel A"),
                period,
                min_percent,
                max_percent,
            },
            Self {
                inner: portions.1.expect("PWM Channel B"),
                period,
                min_percent,
                max_percent,
            },
        )
    }

    pub fn set_pwm_percent(&mut self, percent: f32) -> Result<(), PwmError> {
        let percent = percent.clamp(self.min_percent, self.max_percent);
        let ticks = (self.period as f32 * percent) as u16;
        self.inner.set_duty_cycle(ticks)
    }

    // Axis is [-1..1]
    pub fn set_from_axis_control(&mut self, axis: f32) -> Result<(), PwmError> {
        let f = (axis + 1.0) / 2.0;
        let percent = self.min_percent + f * (self.max_percent - self.min_percent);

        let ticks = (self.period as f32 * percent) as u16;
        self.inner.set_duty_cycle(ticks)
    }
}
