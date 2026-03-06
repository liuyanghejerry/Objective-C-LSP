// UITest fixture: used by GUI tests to verify extension behavior in a real VS Code instance.
// DO NOT modify without updating the corresponding test expectations.

@interface MyClass : NSObject {
    NSString *_name;
    NSInteger _count;
}
@property (nonatomic, strong) id<UITableViewDelegate> delegate;
@end

@implementation MyClass

- (void)loadData {
    NSInteger count = 42;
    dispatch_async(dispatch_get_main_queue(), ^{
        [self updateUI];
    });
}

- (void)updateUI {
}

@end
