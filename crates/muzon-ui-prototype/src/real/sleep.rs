use super::{message::Message, state::RealApp, style as st};
use iced::{
    widget::{button, column, row, text},
    Element, Length,
};
use muzon_core::sleep_timer::SleepChoice;
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let state = app.snapshot.sleep;
    let label = if state.end_of_track {
        "Остановка после текущего трека".into()
    } else if let Some(seconds) = state.remaining_secs {
        format!("До паузы: {}:{:02}", seconds / 60, seconds % 60)
    } else {
        "Таймер выключен".into()
    };
    let mut presets = row![].spacing(8);
    for minutes in [15, 30, 45, 60, 90] {
        presets = presets.push(
            button(text(format!("{minutes} мин")).size(13))
                .padding([10, 14])
                .style(st::secondary)
                .on_press(Message::SetSleep(SleepChoice::Minutes(minutes))),
        );
    }
    st::scrollable(column![text("Таймер сна").size(28), text(label).size(17).color(st::accent()), presets,
        row![button("После текущего трека").padding([10,14]).style(st::secondary).on_press(Message::SetSleep(SleepChoice::EndOfTrack)),
            button("Отменить таймер").padding([10,14]).style(st::quiet).on_press(Message::SetSleep(SleepChoice::Off))].spacing(12),
        text("Таймер поставит музыку на паузу.\nРаботает и при скрытом окне; после перезапуска выключен.").size(13).width(Length::Fill).color(st::muted())
    ].spacing(20).width(Length::Fill)).height(Length::Fill).width(Length::Fill).id(iced::widget::scrollable::Id::new("muzon-sleep")).style(st::scroll).into()
}
