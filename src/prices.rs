use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Days, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Oslo as Cet;
use tokio::sync::watch;

use crate::{
    api::{DayPrices, NordpoolClient, SpotPrice},
    config::Config,
    influxdb2::publish_to_influxdb2,
    time::sleep_until,
};

/// Never retry failed requests less often than this
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Prices we know about, keyed by CET delivery day
#[derive(Debug, Default, Clone)]
pub struct PriceStore {
    days: BTreeMap<NaiveDate, DayPrices>,
}

impl PriceStore {
    /// Returns the price whose interval contains `now`
    pub fn current(&self, now: DateTime<Utc>) -> Option<&SpotPrice> {
        self.days
            .values()
            .flat_map(|day| &day.prices)
            .find(|p| p.start_date <= now && now < p.end_date)
    }

    fn is_complete(&self, date: NaiveDate) -> bool {
        self.days.get(&date).is_some_and(|day| day.is_final)
    }

    /// Returns true if the stored prices changed
    fn insert(&mut self, date: NaiveDate, day: DayPrices) -> bool {
        if self.days.get(&date) == Some(&day) {
            return false;
        }

        self.days.insert(date, day);
        true
    }

    /// Returns true if anything was removed
    fn prune_before(&mut self, date: NaiveDate) -> bool {
        let len = self.days.len();
        self.days.retain(|d, _| *d >= date);
        self.days.len() != len
    }
}

fn cet_date(t: DateTime<Utc>) -> NaiveDate {
    t.with_timezone(&Cet).date_naive()
}

/// When to start polling for prices of the day after `date`
fn poll_window_start(date: NaiveDate, poll_start: NaiveTime) -> DateTime<Utc> {
    Cet.from_local_datetime(&date.and_time(poll_start))
        .earliest()
        // Only possible if poll_start falls in a DST gap, an hour later is fine
        .unwrap_or_else(|| Cet.from_utc_datetime(&date.and_time(poll_start)))
        .with_timezone(&Utc)
}

#[derive(Debug, PartialEq)]
enum Plan {
    Fetch(Vec<NaiveDate>),
    SleepUntil(DateTime<Utc>),
}

fn plan(now: DateTime<Utc>, store: &PriceStore, poll_start: NaiveTime) -> Plan {
    let today = cet_date(now);
    let tomorrow = today + Days::new(1);
    let window_start = poll_window_start(today, poll_start);

    let mut dates = vec![];

    if !store.is_complete(today) {
        dates.push(today);
    }

    if now >= window_start && !store.is_complete(tomorrow) {
        dates.push(tomorrow);
    }

    if !dates.is_empty() {
        Plan::Fetch(dates)
    } else if now < window_start {
        Plan::SleepUntil(window_start)
    } else {
        Plan::SleepUntil(poll_window_start(tomorrow, poll_start))
    }
}

/// Keeps `tx` up to date with today's and (once published) tomorrow's prices
pub async fn run_fetcher(config: Config, tx: watch::Sender<PriceStore>) -> eyre::Result<()> {
    let client = NordpoolClient::new(config.prices.clone())?;
    let poll_interval = Duration::from_secs(config.prices.poll_interval_secs.max(1));
    let mut retry_interval = poll_interval;

    loop {
        let plan = plan(Utc::now(), &tx.borrow(), config.prices.poll_start);
        let dates = match plan {
            Plan::Fetch(dates) => dates,
            Plan::SleepUntil(t) => {
                info!("Prices are up to date, next poll at {t}");
                sleep_until(t).await;
                continue;
            }
        };

        let mut failed = false;

        for date in dates {
            match client.fetch_day(date).await {
                Ok(Some(day)) => {
                    let is_final = day.is_final;
                    let changed = tx.send_if_modified(|store| store.insert(date, day.clone()));

                    if changed {
                        info!(
                            "Fetched {} {} prices for {date}",
                            day.prices.len(),
                            if is_final { "final" } else { "preliminary" },
                        );

                        if let Some(influx_config) = config.influxdb2.clone() {
                            tokio::spawn(async move {
                                let write = publish_to_influxdb2(&day.prices, &influx_config);
                                match tokio::time::timeout(Duration::from_secs(60), write).await {
                                    Ok(Ok(())) => info!("Successfully wrote to InfluxDB"),
                                    Ok(Err(e)) => error!("Error writing to InfluxDB: {e:#}"),
                                    Err(_) => error!("Timed out writing to InfluxDB"),
                                }
                            });
                        }
                    }
                }
                Ok(None) => info!("Prices for {date} have not been published yet"),
                Err(e) => {
                    error!("Error fetching prices for {date}: {e:#}");
                    failed = true;
                }
            }
        }

        let yesterday = cet_date(Utc::now()) - Days::new(1);
        tx.send_if_modified(|store| store.prune_before(yesterday));

        if failed {
            // Back off exponentially on errors so we don't hammer a broken API
            tokio::time::sleep(retry_interval).await;
            retry_interval = (retry_interval * 2).min(MAX_RETRY_INTERVAL.max(poll_interval));
        } else {
            retry_interval = poll_interval;
            tokio::time::sleep(poll_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn day(start: &str, hours: i64, is_final: bool) -> DayPrices {
        let start = t(start);
        DayPrices {
            prices: (0..hours)
                .map(|h| SpotPrice {
                    price: h as f64,
                    start_date: start + chrono::Duration::hours(h),
                    end_date: start + chrono::Duration::hours(h + 1),
                })
                .collect(),
            is_final,
        }
    }

    fn noon() -> NaiveTime {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn fetches_today_when_empty() {
        let store = PriceStore::default();
        assert_eq!(
            plan(t("2026-10-14T05:00:00Z"), &store, noon()),
            Plan::Fetch(vec![d("2026-10-14")])
        );
    }

    #[test]
    fn waits_for_poll_window() {
        let mut store = PriceStore::default();
        store.insert(d("2026-10-14"), day("2026-10-13T22:00:00Z", 24, true));

        // 12:00 CEST is 10:00 UTC
        assert_eq!(
            plan(t("2026-10-14T05:00:00Z"), &store, noon()),
            Plan::SleepUntil(t("2026-10-14T10:00:00Z"))
        );
        assert_eq!(
            plan(t("2026-10-14T10:00:00Z"), &store, noon()),
            Plan::Fetch(vec![d("2026-10-15")])
        );
    }

    #[test]
    fn keeps_polling_preliminary_prices() {
        let mut store = PriceStore::default();
        store.insert(d("2026-10-14"), day("2026-10-13T22:00:00Z", 24, true));
        store.insert(d("2026-10-15"), day("2026-10-14T22:00:00Z", 24, false));

        assert_eq!(
            plan(t("2026-10-14T11:00:00Z"), &store, noon()),
            Plan::Fetch(vec![d("2026-10-15")])
        );

        store.insert(d("2026-10-15"), day("2026-10-14T22:00:00Z", 24, true));

        assert_eq!(
            plan(t("2026-10-14T11:00:00Z"), &store, noon()),
            Plan::SleepUntil(t("2026-10-15T10:00:00Z"))
        );
    }

    #[test]
    fn handles_dst_change() {
        let mut store = PriceStore::default();
        // Last Sunday of October 2026 is 25.10., CET (UTC+1) from then on
        store.insert(d("2026-10-25"), day("2026-10-24T22:00:00Z", 25, true));
        store.insert(d("2026-10-26"), day("2026-10-25T23:00:00Z", 24, true));

        assert_eq!(
            plan(t("2026-10-25T12:00:00Z"), &store, noon()),
            Plan::SleepUntil(t("2026-10-26T11:00:00Z"))
        );
    }

    #[test]
    fn finds_current_price() {
        let mut store = PriceStore::default();
        store.insert(d("2026-10-14"), day("2026-10-13T22:00:00Z", 24, true));

        let now = t("2026-10-14T05:30:00Z");
        let price = store.current(now).unwrap();
        assert_eq!(price.start_date, t("2026-10-14T05:00:00Z"));
        assert_eq!(price.price, 7.0);

        assert!(store.current(t("2026-10-14T22:00:00Z")).is_none());
    }

    #[test]
    fn prunes_old_days() {
        let mut store = PriceStore::default();
        store.insert(d("2026-10-13"), day("2026-10-12T22:00:00Z", 24, true));
        store.insert(d("2026-10-14"), day("2026-10-13T22:00:00Z", 24, true));

        assert!(store.prune_before(d("2026-10-14")));
        assert!(!store.prune_before(d("2026-10-14")));
        assert!(store.is_complete(d("2026-10-14")));
        assert!(!store.is_complete(d("2026-10-13")));
    }
}
