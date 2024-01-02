use dashmap::DashMap;
use lazy_static::lazy_static;
use tokio::sync::{RwLock, OwnedRwLockReadGuard, OwnedRwLockWriteGuard};
use std::sync::Arc;

lazy_static! {
    static ref LOCK_MAP: DashMap<String, Arc<RwLock<()>>> = DashMap::new();
}

pub(crate) async fn lock(s: &str) -> OwnedRwLockWriteGuard<()> {
    let arc_lock = LOCK_MAP.entry(s.to_string())
        .or_insert_with(|| Arc::new(RwLock::new(())))
        .clone();
    // Use `read_owned().await` to get an `OwnedRwLockReadGuard`
    arc_lock.write_owned().await
}

#[cfg(test)]
mod test {
    use std::time::Duration;
    use tokio::time::sleep;
    use tracing::info;
    use crate::{cfg, common};
    use crate::common::lock::lock;

    async fn try_lock_it(i : i32) {

    }
    #[tokio::test]
    async fn test_lock() {
        cfg::init().unwrap();
        common::trace::init();
        let handles: Vec<_> = (0..100).map(|i| {
            let lock_name = if i % 2 == 0 { "even" } else { "odd" };
            tokio::spawn(async move {
                info!("Task {} try get  lock for {}", i, lock_name);
                let _guard = lock(lock_name).await;
                // 在这里执行一些操作，比如访问或修改共享资源。
                info!("Task {} acquired lock for {}", i, lock_name);
                // 模拟一些工作。
                tokio::time::sleep(Duration::from_millis(10)).await;
                info!("Task {} released lock for {}", i, lock_name);
            })
        }).collect();

        // 等待所有任务完成。
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_async() {
        use tokio::sync::mpsc;
        use tokio::time::{sleep, Duration};

        // 假设这是你的用户消息类型
        #[derive(Debug)]
        struct UserMessage {
            user_id: u32,
            content: String,
        }

        // 用户的消息处理器
        async fn user_message_handler(user_id: u32, mut rx: mpsc::Receiver<UserMessage>) {
            while let Some(message) = rx.recv().await {
                println!("用户 {} 正在处理消息: {:?}", user_id, message.content);
                // 在这里处理消息...
                // 模拟消息处理时间
                sleep(Duration::from_millis(100)).await;
            }
        }

        // 假设你有一个用户 ID 列表
        let user_ids = 1..100000;

        // 为每个用户创建一个消息队列和处理器
        for user_id in user_ids {
            // 创建一个 mpsc 通道
            let (tx, rx) = mpsc::channel(100); // 通道的容量可以根据需要调整

            // 启动一个异步任务来处理用户消息
            tokio::spawn(user_message_handler(user_id, rx));

            // 模拟发送消息到用户的队列
            let user_tx = tx.clone();
            tokio::spawn(async move {
                for i in 0..5 {
                    let message = UserMessage {
                        user_id,
                        content: format!("消息内容 {}", i),
                    };
                    user_tx.send(message).await.unwrap();
                    sleep(Duration::from_millis(50)).await;
                }
            });
        }

        // 等待足够的时间让消息处理完成
        sleep(Duration::from_secs(5)).await;

    }

}