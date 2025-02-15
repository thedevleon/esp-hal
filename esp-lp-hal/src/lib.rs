#![allow(asm_sub_register)]
#![no_std]

use core::arch::global_asm;

pub mod delay;
pub mod gpio;
#[cfg(esp32c6)]
pub mod uart;

pub mod pac {
    #[cfg(feature = "esp32c6")]
    pub use esp32c6_lp::*;
    #[cfg(feature = "esp32s2")]
    pub use esp32s2_ulp::*;
    #[cfg(feature = "esp32s3")]
    pub use esp32s3_ulp::*;
}

pub mod prelude {
    pub use procmacros::entry;
}

cfg_if::cfg_if! {
    if #[cfg(feature = "esp32c6")] {
        // LP_FAST_CLK is not very accurate, for now use a rough estimate
        const LP_FAST_CLK_HZ: u32 = 16_000_000;
        const XTAL_D2_CLK_HZ: u32 = 20_000_000;
    } else if #[cfg(feature = "esp32s2")] {
        const LP_FAST_CLK_HZ: u32 = 8_000_000;
    } else if #[cfg(feature = "esp32s3")] {
        const LP_FAST_CLK_HZ: u32 = 17_500_000;
    }
}

pub static mut CPU_CLOCK: u32 = LP_FAST_CLK_HZ;

#[cfg(feature = "esp32c6")]
global_asm!(
    r#"
    .section    .init.vector, "ax"
    /* This is the vector table. It is currently empty, but will be populated
        * with exception and interrupt handlers when this is supported
    */

    .align  0x4, 0xff
    .global _vector_table
    .type _vector_table, @function
_vector_table:
    .option push
    .option norvc

    .rept 32
    nop
    .endr

    .option pop
    .size _vector_table, .-_vector_table

    .section .init, "ax"
    .global reset_vector

/* The reset vector, jumps to startup code */
reset_vector:
    j __start

__start:
    /* setup the stack pointer */
    la sp, __stack_top
    call rust_main
loop:
    j loop
"#
);

#[cfg(any(feature = "esp32s2", feature = "esp32s3"))]
global_asm!(
    r#"
	.section .text.vectors
	.global irq_vector
	.global reset_vector

/* The reset vector, jumps to startup code */
reset_vector:
	j __start

/* Interrupt handler */
.balign 16
irq_vector:
	ret

	.section .text

__start:
    /* setup the stack pointer */
	la sp, __stack_top

	call ulp_riscv_rescue_from_monitor
	call rust_main
	call ulp_riscv_halt
loop:
	j loop
"#
);

#[link_section = ".init.rust"]
#[export_name = "rust_main"]
unsafe extern "C" fn lp_core_startup() -> ! {
    extern "Rust" {
        fn main() -> !;
    }

    #[cfg(feature = "esp32c6")]
    if (*pac::LP_CLKRST::PTR)
        .lp_clk_conf()
        .read()
        .fast_clk_sel()
        .bit_is_set()
    {
        CPU_CLOCK = XTAL_D2_CLK_HZ;
    }

    main();
}

#[cfg(any(feature = "esp32s2", feature = "esp32s3"))]
#[link_section = ".init.rust"]
#[no_mangle]
unsafe extern "C" fn ulp_riscv_rescue_from_monitor() {
    // Rescue RISC-V core from monitor state.
    unsafe { &*pac::RTC_CNTL::PTR }
        .cocpu_ctrl()
        .modify(|_, w| w.cocpu_done().clear_bit().cocpu_shut_reset_en().clear_bit());
}

#[cfg(any(feature = "esp32s2", feature = "esp32s3"))]
#[link_section = ".init.rust"]
#[no_mangle]
unsafe extern "C" fn ulp_riscv_halt() {
    unsafe { &*pac::RTC_CNTL::PTR }.cocpu_ctrl().modify(|_, w| {
        w.cocpu_shut_2_clk_dis()
            .variant(0x3f)
            .cocpu_done()
            .set_bit()
    });

    loop {
        riscv::asm::wfi();
    }
}
