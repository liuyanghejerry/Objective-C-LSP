@interface MyViewController : UIViewController <UITableViewDelegate, UITableViewDataSource>
@property (nonatomic, strong) NSString *title;
@property (nonatomic, weak) id<MyDelegate> delegate;
@end

@implementation MyViewController

- (void)viewDidLoad {
    [super viewDidLoad];
}

- (void)loadData {
    NSInteger count = 42;
    dispatch_async(dispatch_get_main_queue(), ^{
        [self updateUI];
    });
}

- (void)updateUI {
}

@end
