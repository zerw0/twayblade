//! One early process delegate handles both running and cold-launch responses.
//! Unsafe operations are restricted to Objective-C protocol registration and
//! framework-guaranteed callback/dictionary types; application data stays safe Rust.
use super::*;
use objc2::{
    AnyThread, define_class,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::*;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use tokio::sync::oneshot;
const PAYLOAD_KEY: &str = "twayblade.activation.v1";
type Pending = (oneshot::Sender<NotificationResponse>, Activation);
fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(Mutex::default)
}
fn bundled() -> bool {
    mac_usernotifications::check_bundle().is_ok()
}

define_class!(
    // SAFETY: NSObject subclass with no ivars; callbacks use only thread-safe Rust
    // channels and retained framework objects for the duration of each invocation.
    #[unsafe(super(NSObject))]
    #[name = "TwaybladeNotificationDelegate"]
    struct NotificationDelegate;
    unsafe impl NSObjectProtocol for NotificationDelegate {}
    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &block2::DynBlock<dyn Fn()>,
        ) {
            // SAFETY: framework-exported immutable NSString constant.
            let is_default =
                &*response.actionIdentifier() == unsafe { UNNotificationDefaultActionIdentifier };
            let request = response.notification().request();
            let live = pending()
                .lock()
                .ok()
                .and_then(|mut map| map.remove(&request.identifier().to_string()));
            let action = match response.actionIdentifier().to_string().as_str() {
                "twayblade.call.accept" => NotificationAction::Accept,
                "twayblade.call.decline" => NotificationAction::Decline,
                _ => NotificationAction::Open,
            };
            if is_default || action != NotificationAction::Open {
                let delivered = live.is_some_and(|(sender, target)| {
                    sender.send(NotificationResponse { target, action }).is_ok()
                });
                if !delivered {
                    let info = request.content().userInfo();
                    let key = NSString::from_str(PAYLOAD_KEY);
                    if let Some(value) = info.objectForKey(key.as_ref())
                        && let Some(payload) = value.downcast_ref::<NSString>()
                    {
                        super::activation::receive_launch(&payload.to_string());
                    }
                }
            }
            completion.call(());
        }
    }
);
impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: calls NSObject's designated initializer on the freshly allocated subclass.
        unsafe { objc2::msg_send![super(this), init] }
    }
}
pub(super) fn initialize() {
    if !bundled() {
        return;
    }
    static DELEGATE: OnceLock<Retained<NotificationDelegate>> = OnceLock::new();
    DELEGATE.get_or_init(|| {
        let accept = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str("twayblade.call.accept"),
            &NSString::from_str("Accept"),
            UNNotificationActionOptions::Foreground,
        );
        let decline = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str("twayblade.call.decline"),
            &NSString::from_str("Decline"),
            UNNotificationActionOptions::empty(),
        );
        let category =
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &NSString::from_str("twayblade.call"),
                &NSArray::from_slice(&[&*accept, &*decline]),
                &NSArray::new(),
                UNNotificationCategoryOptions::empty(),
            );
        UNUserNotificationCenter::currentNotificationCenter()
            .setNotificationCategories(&objc2_foundation::NSSet::from_slice(&[&*category]));
        let delegate = NotificationDelegate::new();
        UNUserNotificationCenter::currentNotificationCenter()
            .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        delegate
    });
}
fn begin_permission_read() -> oneshot::Receiver<Permission> {
    let (tx, rx) = oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let callback =
        block2::RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
            // SAFETY: UserNotifications guarantees a live settings object during this callback.
            let status = unsafe { settings.as_ref() }.authorizationStatus();
            let value = match status {
                UNAuthorizationStatus::Authorized
                | UNAuthorizationStatus::Provisional
                | UNAuthorizationStatus::Ephemeral => Permission::Granted,
                UNAuthorizationStatus::Denied => Permission::Denied,
                UNAuthorizationStatus::NotDetermined => Permission::NotRequested,
                _ => Permission::Unavailable,
            };
            if let Ok(mut tx) = tx.lock()
                && let Some(tx) = tx.take()
            {
                let _ = tx.send(value);
            }
        });
    UNUserNotificationCenter::currentNotificationCenter()
        .getNotificationSettingsWithCompletionHandler(&callback);
    rx
}
pub async fn permission() -> Permission {
    if !bundled() {
        return Permission::Unavailable;
    }
    initialize();
    begin_permission_read()
        .await
        .unwrap_or(Permission::Unavailable)
}
fn begin_permission_request() -> oneshot::Receiver<bool> {
    let (tx, rx) = oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let callback =
        block2::RcBlock::new(move |granted: objc2::runtime::Bool, _error: *mut NSError| {
            if let Ok(mut tx) = tx.lock()
                && let Some(tx) = tx.take()
            {
                let _ = tx.send(granted.as_bool());
            }
        });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &callback,
        );
    rx
}
pub async fn request_permission() -> Permission {
    if !bundled() {
        return Permission::Unavailable;
    }
    initialize();
    match begin_permission_request().await {
        Ok(true) => Permission::Granted,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unavailable,
    }
}
fn begin_notify(
    id: &str,
    notification: &Notification,
    call: bool,
) -> Result<oneshot::Receiver<bool>, NotificationError> {
    let content = UNMutableNotificationContent::new();
    if call {
        content.setCategoryIdentifier(&NSString::from_str("twayblade.call"));
    }
    content.setTitle(&NSString::from_str(&notification.title));
    content.setBody(&NSString::from_str(&notification.body));
    if notification.sound {
        content.setSound(Some(&UNNotificationSound::defaultSound()));
    }
    if let Some(launch) = &notification.launch {
        let key = NSString::from_str(PAYLOAD_KEY);
        let payload = NSString::from_str(&launch.encode()?);
        let info = NSDictionary::<NSString, NSString>::from_slices(&[&*key], &[&*payload]);
        // SAFETY: this dictionary contains only NSString keys and values, which
        // satisfy UserNotifications' property-list requirement. Erasure widens
        // the static object types; it does not reinterpret the stored objects.
        unsafe {
            content.setUserInfo(info.cast_unchecked::<AnyObject, AnyObject>());
        }
    }
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(id),
        &content,
        None,
    );
    let (tx, rx) = oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let callback = block2::RcBlock::new(move |error: *mut NSError| {
        if let Ok(mut tx) = tx.lock()
            && let Some(tx) = tx.take()
        {
            let _ = tx.send(error.is_null());
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, Some(&callback));
    Ok(rx)
}
pub async fn notify(
    id: String,
    notification: Notification,
    call: bool,
) -> Result<PendingNotification, NotificationError> {
    if !bundled() {
        return Err(NotificationError);
    }
    initialize();
    let (sender, receiver) = oneshot::channel();
    {
        let mut map = pending().lock().map_err(|_| NotificationError)?;
        if map.len() >= 64
            && let Some(old) = map.keys().next().cloned()
        {
            map.remove(&old);
        }
        map.insert(id.clone(), (sender, notification.target.clone()));
    }
    let result = begin_notify(&id, &notification, call)?
        .await
        .unwrap_or(false);
    if !result {
        if let Ok(mut map) = pending().lock() {
            map.remove(&id);
        }
        return Err(NotificationError);
    }
    Ok(PendingNotification {
        id,
        response: Box::pin(async move { receiver.await.ok() }),
    })
}
pub async fn dismiss(id: &str) {
    if let Ok(mut map) = pending().lock() {
        map.remove(id);
    }
    if bundled() {
        UNUserNotificationCenter::currentNotificationCenter()
            .removeDeliveredNotificationsWithIdentifiers(&NSArray::from_slice(&[
                &*NSString::from_str(id),
            ]));
    }
}
pub(super) fn clear_delivered() {
    if let Ok(mut map) = pending().lock() {
        map.clear();
    }
    if bundled() {
        UNUserNotificationCenter::currentNotificationCenter().removeAllDeliveredNotifications();
    }
}
pub async fn set_badge(count: u64) -> Result<(), NotificationError> {
    if bundled() {
        UNUserNotificationCenter::currentNotificationCenter()
            .setBadgeCount_withCompletionHandler(count.min(isize::MAX as u64) as isize, None);
    }
    Ok(())
}
pub fn open_settings() -> Result<(), NotificationError> {
    std::process::Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.twayblade.desktop.development").spawn().map(|_|()).map_err(|_|NotificationError)
}
